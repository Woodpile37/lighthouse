use std::sync::Arc;

use libp2p::core::connection::ConnectionId;
use types::{BlobsSidecar, EthSpec, SignedBeaconBlock};

use crate::rpc::methods::{BlobsByRangeRequest, BlobsByRootRequest};
use crate::rpc::{
    methods::{
        BlocksByRangeRequest, BlocksByRootRequest, OldBlocksByRangeRequest, RPCCodedResponse,
        RPCResponse, ResponseTermination, StatusMessage,
    },
    OutboundRequest, SubstreamId,
};
use types::SignedBeaconBlockAndBlobsSidecar;

/// Identifier of requests sent by a peer.
pub type PeerRequestId = (ConnectionId, SubstreamId);

/// Identifier of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestId<AppReqId> {
    Application(AppReqId),
    Internal,
}

/// The type of RPC requests the Behaviour informs it has received and allows for sending.
///
// NOTE: This is an application-level wrapper over the lower network level requests that can be
//       sent. The main difference is the absence of the Ping, Metadata and Goodbye protocols, which don't
//       leave the Behaviour. For all protocols managed by RPC see `RPCRequest`.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// A Status message.
    Status(StatusMessage),
    /// A blocks by range request.
    BlocksByRange(BlocksByRangeRequest),
    /// A blobs by range request.
    BlobsByRange(BlobsByRangeRequest),
    /// A request blocks root request.
    BlocksByRoot(BlocksByRootRequest),
    /// A request blobs root request.
    BlobsByRoot(BlobsByRootRequest),
}

impl<TSpec: EthSpec> std::convert::From<Request> for OutboundRequest<TSpec> {
    fn from(req: Request) -> OutboundRequest<TSpec> {
        match req {
            Request::BlocksByRoot(r) => OutboundRequest::BlocksByRoot(r),
            Request::BlocksByRange(BlocksByRangeRequest { start_slot, count }) => {
                OutboundRequest::BlocksByRange(OldBlocksByRangeRequest {
                    start_slot,
                    count,
                    step: 1,
                })
            }
            Request::BlobsByRange(r) => OutboundRequest::BlobsByRange(r),
            Request::BlobsByRoot(r) => OutboundRequest::BlobsByRoot(r),
            Request::Status(s) => OutboundRequest::Status(s),
        }
    }
}

/// The type of RPC responses the Behaviour informs it has received, and allows for sending.
///
// NOTE: This is an application-level wrapper over the lower network level responses that can be
//       sent. The main difference is the absense of Pong and Metadata, which don't leave the
//       Behaviour. For all protocol reponses managed by RPC see `RPCResponse` and
//       `RPCCodedResponse`.
#[derive(Debug, Clone, PartialEq)]
pub enum Response<TSpec: EthSpec> {
    /// A Status message.
    Status(StatusMessage),
    /// A response to a get BLOCKS_BY_RANGE request. A None response signals the end of the batch.
    BlocksByRange(Option<Arc<SignedBeaconBlock<TSpec>>>),
    /// A response to a get BLOBS_BY_RANGE request. A None response signals the end of the batch.
    BlobsByRange(Option<Arc<BlobsSidecar<TSpec>>>),
    /// A response to a get BLOCKS_BY_ROOT request.
    BlocksByRoot(Option<Arc<SignedBeaconBlock<TSpec>>>),
    /// A response to a get BLOBS_BY_ROOT request.
    BlobsByRoot(Option<SignedBeaconBlockAndBlobsSidecar<TSpec>>),
}

impl<TSpec: EthSpec> std::convert::From<Response<TSpec>> for RPCCodedResponse<TSpec> {
    fn from(resp: Response<TSpec>) -> RPCCodedResponse<TSpec> {
        match resp {
            Response::BlocksByRoot(r) => match r {
                Some(b) => RPCCodedResponse::Success(RPCResponse::BlocksByRoot(b)),
                None => RPCCodedResponse::StreamTermination(ResponseTermination::BlocksByRoot),
            },
            Response::BlocksByRange(r) => match r {
                Some(b) => RPCCodedResponse::Success(RPCResponse::BlocksByRange(b)),
                None => RPCCodedResponse::StreamTermination(ResponseTermination::BlocksByRange),
            },
            Response::BlobsByRoot(r) => match r {
                Some(b) => RPCCodedResponse::Success(RPCResponse::BlockAndBlobsByRoot(b)),
                None => RPCCodedResponse::StreamTermination(ResponseTermination::BlobsByRoot),
            },
            Response::BlobsByRange(r) => match r {
                Some(b) => RPCCodedResponse::Success(RPCResponse::BlobsByRange(b)),
                None => RPCCodedResponse::StreamTermination(ResponseTermination::BlobsByRange),
            },
            Response::Status(s) => RPCCodedResponse::Success(RPCResponse::Status(s)),
        }
    }
}

impl<AppReqId: std::fmt::Debug> slog::Value for RequestId<AppReqId> {
    fn serialize(
        &self,
        record: &slog::Record,
        key: slog::Key,
        serializer: &mut dyn slog::Serializer,
    ) -> slog::Result {
        match self {
            RequestId::Internal => slog::Value::serialize("Behaviour", record, key, serializer),
            RequestId::Application(ref id) => {
                slog::Value::serialize(&format_args!("{:?}", id), record, key, serializer)
            }
        }
    }
}
