use tokio::time::{Duration, sleep};
use crate::node::config::Address;
use crate::node::peer::Peer;
use anyhow::Result;
use futures_util::future::join_all;
use log::{error, info, warn};
use rand::Rng;
use tokio::sync::RwLock;
use crate::node::raft_rpc::VoteRequest;

const ELECTION_TIMEOUT_MIN: u32 = 150;
const ELECTION_TIMEOUT_MAX: u32 = 300;

#[derive(Debug)]
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
}

impl Membership {

    pub fn new(node_id: String, nodes: Vec<Address>, heartbeat_interval: u64) -> Self {
        return Self {
            node_id,
            current_term: RwLock::new(0),
            state: RwLock::new(State::Follower),
            nodes,
            peers: RwLock::new(vec![]),
            voted_for: RwLock::new(None),
            heartbeat_interval,
        };
    }

    pub async fn start(&self) -> Result<()> {
        let peers = &mut self.connect_to_peers().await?;
        self.update_peers(peers).await;
        sleep(Duration::from_millis(self.get_election_timeout())).await;
        self.start_election().await;

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

    async fn start_election(&self) {
        let election_timeout = self.get_election_timeout();
        self.increment_current_term().await;
        self.set_state(State::Candidate).await;
        self.set_voted_for(Some(self.node_id.clone())).await;  // vote for ourselves
        let term = self.current_term.read().await;

        let peers = self.peers.read().await;
        loop {
            let futures = peers.iter().map(|peer| {
                let term = *term;
                let node_id = self.node_id.clone();
                peer.request_vote(election_timeout, term, node_id)
            });
            let results = join_all(futures).await;

            // add ourselves
            let total_nodes = peers.len() + 1;
            // add 1 since we voted for ourselves
            let votes: usize = 1 + results.iter().fold(0, |sum, x| {
                if x.is_ok() {
                    if x.as_ref().unwrap().vote_granted { 1 } else { 0 }
                } else {
                    warn!("{}", x.as_ref().err().unwrap());
                    0
                }
            });
            let votes_needed = (total_nodes / 2) + 1;

            if votes >= votes_needed {
                info!("Elected!");
                break;
            }

        }

    }

    pub async fn grant_vote(&self, vote_request: VoteRequest) -> (bool, u32) {
        let current_term;
        {
            current_term = (*self.current_term.read().await).clone();
        }
        if vote_request.term < current_term {
            return (false, current_term);
        }

        let voted_for;
        {
           voted_for = (*self.voted_for.read().await).clone();
        }
        if let Some(candidate_id) = voted_for {
            if candidate_id != vote_request.candidate_id {
                return (false, current_term);
            }

            // todo: log checks
        }

        self.set_voted_for(Some(vote_request.candidate_id)).await;
        self.set_current_term(vote_request.term).await;
        self.set_state(State::Follower).await;
        return (true, current_term);
    }

    fn get_election_timeout(&self) -> u64 {
        return rand::thread_rng().gen_range(
            ELECTION_TIMEOUT_MIN..=ELECTION_TIMEOUT_MAX
        ) as u64;
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
}
