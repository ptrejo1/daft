use std::sync::Arc;
use tonic::{Request, Response, Status};
use tonic::transport::Server;
use crate::node::config::Address;
use crate::node::membership::Membership;
use crate::node::raft_rpc::{VoteReply, VoteRequest, AppendEntriesRequest, AppendEntriesReply};
use crate::node::raft_rpc::raft_server::{Raft, RaftServer};
use anyhow::Result;
use log::info;

#[derive(Debug)]
pub struct PeerServer {
    address: Address,
    membership: Arc<Membership>,
}

impl PeerServer {

    pub fn new(address: Address, membership: Arc<Membership>) -> Self {
        return Self { address, membership: membership.clone() };
    }

    pub async fn start(&self) -> Result<()> {
        let addr = self.address.to_string().parse()?;
        let raft_service = RaftService {
            membership: self.membership.clone()
        };

        info!("Starting peer server at {}", addr);
        Server::builder()
            .add_service(RaftServer::new(raft_service))
            .serve(addr)
            .await?;

        Ok(())
    }
}

#[derive(Debug)]
struct RaftService {
    membership: Arc<Membership>,
}

#[tonic::async_trait]
impl Raft for RaftService {
    async fn request_vote(
        &self,
        request: Request<VoteRequest>
    ) -> Result<Response<VoteReply>, Status> {
        let vote_request = request.into_inner();
        info!("VoteRequest from: {}", vote_request.candidate_id);

        let (vote_granted, current_term) = self.membership.grant_vote(vote_request).await;
        let reply = VoteReply {
            term: current_term,
            vote_granted,
        };

        Ok(Response::new(reply))
    }

    async fn append_entries(
        &self,
        request: Request<AppendEntriesRequest>
    ) -> Result<Response<AppendEntriesReply>, Status> {
        let append_entries_request = request.into_inner();
        info!("AppendEntriesRequest");

        let current_term = self.membership.current_term.read().await.clone();
        let success = append_entries_request.term < current_term;

        // empty signifies a heartbeat
        if append_entries_request.entries.is_empty() {
            self.membership.reset_election_timeout().await;
        }

        Ok(Response::new(AppendEntriesReply {
            term: current_term,
            success,
        }))
    }
}
