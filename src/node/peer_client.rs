use std::time::Duration;
use crate::node::config::Address;
use anyhow::Result;
use tonic::transport::Channel;
use crate::node::raft_rpc::raft_client::RaftClient;
use crate::node::raft_rpc::{AppendEntriesReply, AppendEntriesRequest, VoteReply, VoteRequest};

#[derive(Debug)]
pub struct PeerClient {
    raft_client: RaftClient<Channel>,
}

impl PeerClient {

    pub async fn new(address: &Address) -> Result<Self> {
        let addr = format!("http://{}", address.to_string());
        let raft_client = RaftClient::connect(addr).await?;

        Ok(Self { raft_client })
    }

     pub async fn request_vote(
         &self,
         timeout: u64,
         term: u32,
         candidate_id: String,
         last_log_index: u32,
         last_log_term: u32
     ) -> Result<VoteReply> {
         let mut request = tonic::Request::new(VoteRequest {
             term, candidate_id, last_log_index, last_log_term
         });
         request.set_timeout(Duration::from_millis(timeout));
         let mut client = self.raft_client.clone();
         let response = client.request_vote(request).await?;
         let reply = response.into_inner();

         Ok(reply)
     }

    pub async fn append_entries(
        &self,
        timeout: u64,
        term: u32,
        leader_id: String,
        prev_log_index: u32,
        prev_log_term: u32,
        entries: Vec<String>,
        leader_commit: u32
    ) -> Result<AppendEntriesReply> {
        let mut request = tonic::Request::new(AppendEntriesRequest {
            term,
            leader_id,
            prev_log_index,
            prev_log_term,
            entries,
            leader_commit,
        });
        request.set_timeout(Duration::from_millis(timeout));
        let mut client = self.raft_client.clone();
        let response = client.append_entries(request).await?;
        let reply = response.into_inner();

        Ok(reply)
    }
}
