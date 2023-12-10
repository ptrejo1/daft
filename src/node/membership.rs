use std::time::SystemTime;
use tokio::time::{Duration, sleep};
use crate::node::config::Address;
use crate::node::peer::Peer;
use anyhow::Result;
use futures_util::future::join_all;
use log::{error, info, warn};
use rand::Rng;
use tokio::sync::RwLock;
use crate::node::membership::State::{Candidate, Follower, Leader};
use crate::node::raft_rpc::VoteRequest;

const ELECTION_TIMEOUT_MIN: u32 = 150;
const ELECTION_TIMEOUT_MAX: u32 = 300;

type UnixTimestampMilli = u128;

#[derive(Debug, Clone)]
pub enum State {
    Leader,
    Follower,
    Candidate,
}

#[derive(Debug)]
pub struct Membership {
    node_id: String,
    pub current_term: RwLock<u32>,
    pub state: RwLock<State>,
    nodes: Vec<Address>,
    peers: RwLock<Vec<Peer>>,
    voted_for: RwLock<Option<String>>,
    heartbeat_interval: u64,
    election_timeout: RwLock<UnixTimestampMilli>,
}

impl Membership {

    pub fn new(node_id: String, nodes: Vec<Address>, heartbeat_interval: u64) -> Self {
        let timeout = Membership::get_new_timeout();

        return Self {
            node_id,
            current_term: RwLock::new(0),
            state: RwLock::new(Follower),
            nodes,
            peers: RwLock::new(vec![]),
            voted_for: RwLock::new(None),
            heartbeat_interval,
            election_timeout: RwLock::new(timeout),
        };
    }

    pub async fn start(&self) -> Result<()> {
        let peers = &mut self.connect_to_peers().await?;
        self.update_peers(peers).await;
        self.run().await;

        Ok(())
    }

    async fn connect_to_peers(&self) -> Result<Vec<Peer>> {
        info!("Attempting to connect to peers...");
        loop {
            let futures = self.nodes.iter()
                .map(|x| Peer::new(x.clone()));
            let results: Vec<Result<Peer>> = join_all(futures).await;
            let all_success = results.iter().all(|x| x.is_ok());

            if all_success {
                let mut peers: Vec<Peer> = vec![];
                for result in results {
                    peers.push(result.unwrap());
                }

                info!("Connected to peers");
                return Ok(peers);
            } else {
                for x in self.nodes.iter().zip(results) {
                    let (addr, res) = x;
                    if res.is_ok() { continue; }
                    error!(
                        "Couldn't connect to {} - {}",
                        addr.to_string(), res.err().unwrap()
                    );
                }
            }

            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn run(&self) {
        loop {
            let current_state = self.state.read().await.clone();
            let new_state = match current_state {
                Leader => {
                    self.send_heartbeat_loop().await
                }
                Follower => {
                    self.monitor_heartbeat_loop().await
                }
                Candidate => {
                    self.start_election().await
                }
            };
            self.set_state(new_state).await;
        }
    }

    async fn monitor_heartbeat_loop(&self) -> State {
        info!("Monitoring heartbeat");
        loop {
            let timeout = self.election_timeout.read().await.clone();
            let current_time = Membership::current_timestamp_millis();
            if current_time >= timeout {
                return Candidate;
            }

            let until_next_timeout = timeout - current_time;
            sleep(Duration::from_millis(until_next_timeout as u64)).await;
        }
    }

    async fn send_heartbeat_loop(&self) -> State {
        info!("Starting heartbeat");
        loop {
            let peers = self.peers.read().await;
            let current_term = self.current_term.read().await.clone();
            let request_timeout = self.heartbeat_interval / 2;
            let futures = peers.iter().map(|peer| {
                let term = current_term.clone();
                let node_id = self.node_id.clone();
                peer.append_entries(request_timeout, term, node_id)
            });
            let _ = join_all(futures).await;

            sleep(Duration::from_millis(self.heartbeat_interval)).await;
        }
    }

    async fn start_election(&self) -> State {
        info!("Starting election...");
        let next_timeout = self.reset_election_timeout().await;
        self.increment_current_term().await;
        self.set_voted_for(Some(self.node_id.clone())).await;  // vote for ourselves
        let current_term = self.current_term.read().await.clone();

        let peers = self.peers.read().await;
        let request_timeout = next_timeout - Membership::current_timestamp_millis();
        let futures = peers.iter().map(|peer| {
            let term = current_term.clone();
            let node_id = self.node_id.clone();
            peer.request_vote(request_timeout as u64, term, node_id)
        });
        let results = join_all(futures).await;

        // It's possible we received an rpc and were converted to a Follower while
        // waiting for votes, so we need to check that we're still a candidate before
        // tallying the votes
        let current_state = self.state.read().await.clone();
        if matches!(current_state, Follower) {
            return Follower;
        }

        let total_nodes = peers.len() + 1;  // add ourselves
        let mut votes: usize = 1;   // init w/ 1 since we voted for ourselves
        for result in results {
            if let Some(err) = result.as_ref().err() {
                warn!("{}", err);
                continue;
            }

            let reply = result.ok().unwrap();
            // step down if we get a term greater than ours
            if reply.term > current_term {
                self.set_current_term(reply.term).await;
                return Follower;
            }

            if reply.vote_granted { votes += 1; }
        }

        let votes_needed = (total_nodes / 2) + 1;
        return if votes >= votes_needed {
            info!("Elected as leader");
            Leader
        } else {
            // sleep for remaining election period before returning
            let current_time = Membership::current_timestamp_millis();
            let remaining = if current_time > next_timeout {
                0
            } else {
                next_timeout - current_time
            };
            sleep(Duration::from_millis(remaining as u64)).await;
            Candidate
        }
    }

    pub async fn grant_vote(&self, vote_request: VoteRequest) -> (bool, u32) {
        let current_term = self.current_term.read().await.clone();
        if vote_request.term < current_term {
            return (false, current_term);
        }

        let voted_for = self.voted_for.read().await.clone();
        if let Some(voted_for_candidate_id) = voted_for {
            // already voted for someone else in this election
            if current_term == vote_request.term && voted_for_candidate_id != vote_request.candidate_id {
                return (false, current_term);
            }
        }

        // todo: log checks

        self.set_voted_for(Some(vote_request.candidate_id)).await;
        self.set_current_term(vote_request.term).await;
        self.reset_election_timeout().await;
        return (true, current_term);
    }

    async fn update_peers(&self, peers: &mut Vec<Peer>) {
        let mut current = self.peers.write().await;
        current.clear();
        current.append(peers);
    }

    async fn set_voted_for(&self, value: Option<String>) {
        let mut voted_for = self.voted_for.write().await;
        *voted_for = value;
    }

    async fn set_current_term(&self, value: u32) {
        let mut current_term = self.current_term.write().await;
        *current_term = value;
    }

    async fn increment_current_term(&self) {
        let mut current_term = self.current_term.write().await;
        *current_term += 1;
    }

    async fn set_state(&self, value: State) {
        let mut state = self.state.write().await;
        *state = value;
    }

    fn current_timestamp_millis() -> UnixTimestampMilli {
        return SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
    }

    fn get_new_timeout() -> UnixTimestampMilli {
        let ts = Membership::current_timestamp_millis();
        let timeout = rand::thread_rng().gen_range(
            ELECTION_TIMEOUT_MIN..=ELECTION_TIMEOUT_MAX
        );

        return ts + timeout as u128;
    }

    pub async fn reset_election_timeout(&self) -> UnixTimestampMilli {
        let timeout = Membership::get_new_timeout();
        let mut election_timeout = self.election_timeout.write().await;
        *election_timeout = timeout;

        return timeout;
    }
}
