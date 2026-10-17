//! Arrow Flight client: `std.table.read_flight` and `write_flight`.
//!
//! Flight moves Arrow record batches over gRPC. `read_flight` asks a server for the data behind a
//! ticket (`DoGet`); `write_flight` sends a table to a server under a path (`DoPut`). The calls
//! block, and run on their own short-lived thread with their own Tokio runtime so they work from
//! any calling context, as `std.table.sql` does.

use arrow::record_batch::RecordBatch;
use arrow_flight::decode::FlightRecordBatchStream;
use arrow_flight::encode::FlightDataEncoderBuilder;
use arrow_flight::{FlightClient, FlightDescriptor, Ticket};
use futures::TryStreamExt;
use tonic::transport::{Channel, Endpoint};

use crate::interp::RuntimeError;

/// `grpc://host:port` and `grpc+tls://host:port` are the usual Flight spellings; `http://` and
/// `https://` work too.
fn endpoint(url: &str) -> Result<String, RuntimeError> {
    let (scheme, rest) = url.split_once("://").ok_or_else(|| {
        RuntimeError::new(format!(
            "`{url}` is not a Flight address; write it as grpc://host:port or grpc+tls://host:port"
        ))
    })?;
    let scheme = match scheme {
        "grpc" | "grpc+tcp" | "http" => "http",
        "grpc+tls" | "https" => "https",
        other => {
            return Err(RuntimeError::new(format!(
                "unknown Flight scheme `{other}`; use grpc://, grpc+tls://, http:// or https://"
            )));
        }
    };
    Ok(format!("{scheme}://{rest}"))
}

async fn connect(url: &str) -> Result<FlightClient, RuntimeError> {
    let channel: Channel = Endpoint::from_shared(endpoint(url)?)
        .map_err(|e| RuntimeError::new(format!("Flight address `{url}`: {e}")))?
        .connect()
        .await
        .map_err(|e| RuntimeError::new(format!("cannot connect to Flight server `{url}`: {e}")))?;
    Ok(FlightClient::new(channel))
}

/// Runs `fut` to completion on a fresh thread and runtime.
fn block_on<T: Send>(
    fut: impl std::future::Future<Output = Result<T, RuntimeError>> + Send,
) -> Result<T, RuntimeError> {
    std::thread::scope(|s| {
        s.spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| RuntimeError::new(format!("Flight runtime: {e}")))?
                .block_on(fut)
        })
        .join()
        .unwrap_or_else(|_| Err(RuntimeError::new("the Flight client panicked")))
    })
}

/// The record batches the server returns for `ticket`.
pub(crate) fn read(url: &str, ticket: &str) -> Result<Vec<RecordBatch>, RuntimeError> {
    block_on(async {
        let mut client = connect(url).await?;
        let stream: FlightRecordBatchStream = client
            .do_get(Ticket::new(ticket.to_string()))
            .await
            .map_err(|e| RuntimeError::new(format!("Flight DoGet `{ticket}`: {e}")))?;
        stream
            .try_collect()
            .await
            .map_err(|e| RuntimeError::new(format!("Flight DoGet `{ticket}`: {e}")))
    })
}

/// Sends `batch` to the server under `path`.
pub(crate) fn write(url: &str, path: &str, batch: &RecordBatch) -> Result<(), RuntimeError> {
    block_on(async {
        let mut client = connect(url).await?;
        let data = FlightDataEncoderBuilder::new()
            .with_flight_descriptor(Some(FlightDescriptor::new_path(vec![path.to_string()])))
            .build(futures::stream::iter([Ok(batch.clone())]));
        let mut acks = client
            .do_put(data)
            .await
            .map_err(|e| RuntimeError::new(format!("Flight DoPut `{path}`: {e}")))?;
        while acks
            .try_next()
            .await
            .map_err(|e| RuntimeError::new(format!("Flight DoPut `{path}`: {e}")))?
            .is_some()
        {}
        Ok(())
    })
}
