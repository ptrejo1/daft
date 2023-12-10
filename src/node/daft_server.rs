use std::sync::Arc;
use crate::node::config::Config;
use crate::node::membership::Membership;
use crate::node::peer_server::PeerServer;

pub struct DaftServer {
    node_id: String,
    config: Config,
    peer_server: Arc<PeerServer>,
    membership: Arc<Membership>,
}

impl DaftServer {

    pub fn new(config: Config) -> Self {
        let membership = Arc::new(
            Membership::new(
                config.address.to_string(),
                config.nodes.clone(),
                config.heartbeat_interval,
            )
        );
        let peer_server = Arc::new(
            PeerServer::new(
                config.address.clone(),
                membership.clone()
            )
        );

        return Self {
            node_id: config.address.to_string(),
            config: config.clone(),
            peer_server: peer_server.clone(),
            membership: membership.clone()
        };
    }

    pub async fn start(&self) {
        tokio::join!(
            self.peer_server.start(),
            self.membership.start(),
        );
    }
}
