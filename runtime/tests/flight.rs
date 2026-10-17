//! Arrow Flight client against an in-memory server started by the test.
#![cfg(feature = "flight")]

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use arrow::array::{Float32Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use arrow_flight::encode::FlightDataEncoderBuilder;
use arrow_flight::flight_service_server::{FlightService, FlightServiceServer};
use arrow_flight::{
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo,
    HandshakeRequest, HandshakeResponse, PollInfo, PutResult, SchemaResult, Ticket,
};
use futures::{Stream, StreamExt, TryStreamExt};
use telvax_runtime::{Interp, Value};
use tonic::{Request, Response, Status, Streaming};

type Boxed<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send + 'static>>;

/// Keeps what clients put and serves it back by ticket.
#[derive(Clone, Default)]
struct Store(Arc<Mutex<HashMap<String, Vec<RecordBatch>>>>);

#[tonic::async_trait]
impl FlightService for Store {
    type HandshakeStream = Boxed<HandshakeResponse>;
    type ListFlightsStream = Boxed<FlightInfo>;
    type DoGetStream = Boxed<FlightData>;
    type DoPutStream = Boxed<PutResult>;
    type DoActionStream = Boxed<arrow_flight::Result>;
    type ListActionsStream = Boxed<ActionType>;
    type DoExchangeStream = Boxed<FlightData>;

    async fn do_get(&self, req: Request<Ticket>) -> Result<Response<Self::DoGetStream>, Status> {
        let ticket = String::from_utf8_lossy(&req.get_ref().ticket).to_string();
        let batches = self
            .0
            .lock()
            .unwrap()
            .get(&ticket)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("no data for `{ticket}`")))?;
        let stream = FlightDataEncoderBuilder::new()
            .build(futures::stream::iter(batches.into_iter().map(Ok)))
            .map_err(Status::from);
        Ok(Response::new(Box::pin(stream)))
    }

    async fn do_put(
        &self,
        req: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoPutStream>, Status> {
        let mut incoming = req.into_inner();
        // The path travels in the descriptor of the first message.
        let first = incoming
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("DoPut sent nothing"))?;
        let path = first
            .flight_descriptor
            .as_ref()
            .and_then(|d| d.path.first().cloned())
            .ok_or_else(|| Status::invalid_argument("DoPut needs a path descriptor"))?;
        let rest = incoming.map_err(arrow_flight::error::FlightError::from);
        let stream = futures::stream::iter([Ok(first)]).chain(rest);
        let mut decoded =
            arrow_flight::decode::FlightRecordBatchStream::new_from_flight_data(stream);
        let mut batches = Vec::new();
        while let Some(b) = decoded.try_next().await.map_err(Status::from)? {
            batches.push(b);
        }
        self.0.lock().unwrap().insert(path, batches);
        Ok(Response::new(Box::pin(futures::stream::empty())))
    }

    async fn handshake(
        &self,
        _: Request<Streaming<HandshakeRequest>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        Err(Status::unimplemented("handshake"))
    }
    async fn list_flights(
        &self,
        _: Request<Criteria>,
    ) -> Result<Response<Self::ListFlightsStream>, Status> {
        Err(Status::unimplemented("list_flights"))
    }
    async fn get_flight_info(
        &self,
        _: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        Err(Status::unimplemented("get_flight_info"))
    }
    async fn poll_flight_info(
        &self,
        _: Request<FlightDescriptor>,
    ) -> Result<Response<PollInfo>, Status> {
        Err(Status::unimplemented("poll_flight_info"))
    }
    async fn get_schema(
        &self,
        _: Request<FlightDescriptor>,
    ) -> Result<Response<SchemaResult>, Status> {
        Err(Status::unimplemented("get_schema"))
    }
    async fn do_action(
        &self,
        _: Request<Action>,
    ) -> Result<Response<Self::DoActionStream>, Status> {
        Err(Status::unimplemented("do_action"))
    }
    async fn list_actions(
        &self,
        _: Request<Empty>,
    ) -> Result<Response<Self::ListActionsStream>, Status> {
        Err(Status::unimplemented("list_actions"))
    }
    async fn do_exchange(
        &self,
        _: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoExchangeStream>, Status> {
        Err(Status::unimplemented("do_exchange"))
    }
}

/// Starts the server on a free port in its own thread and returns its address.
fn serve(store: Store) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let incoming = tonic::transport::server::TcpIncoming::from(
                    tokio::net::TcpListener::from_std(listener).unwrap(),
                );
                tonic::transport::Server::builder()
                    .add_service(FlightServiceServer::new(store))
                    .serve_with_incoming(incoming)
                    .await
                    .unwrap();
            });
    });
    format!("grpc://{addr}")
}

const SRC: &str = r#"
type Row:
    id: Int
    score: f32
    name: Text

fn fetch(url: Text, ticket: Text) -> Table[Row]:
    std.table.read_flight(url, ticket)

fn send(t: Table[Row], url: Text, path: Text):
    std.table.write_flight(t, url, path)

fn count(t: Table[Row]) -> u64:
    t.len()
"#;

fn load() -> Interp {
    let (ast, perrs) = telvax_parser::parse(SRC);
    assert!(perrs.is_empty(), "parse: {perrs:?}");
    let (hir, diags) = telvax_hir::lower(&ast.unwrap());
    assert!(
        diags.is_empty(),
        "check: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    Interp::new(hir).expect("globals should initialise")
}

#[test]
fn flight_reads_and_writes_tables() {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("score", DataType::Float32, false),
        Field::new("name", DataType::Utf8, false),
    ]));
    let batch = |ids: Vec<i64>, scores: Vec<f32>, names: Vec<&str>| {
        RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(Float32Array::from(scores)),
                Arc::new(StringArray::from(names)),
            ],
        )
        .unwrap()
    };
    let store = Store::default();
    // Two batches under one ticket arrive as one table.
    store.0.lock().unwrap().insert(
        "scores".into(),
        vec![
            batch(vec![1, 2], vec![0.5, 1.5], vec!["a", "b"]),
            batch(vec![3], vec![2.5], vec!["c"]),
        ],
    );
    let url = serve(store.clone());
    let i = load();

    let t = i
        .call("fetch", vec![Value::str(url.clone()), Value::str("scores")])
        .unwrap();
    assert!(matches!(
        i.call("count", vec![t.clone()]).unwrap(),
        Value::Int(3)
    ));
    let Value::Table(table) = &t else { panic!() };
    assert_eq!(
        table.row(2).unwrap().to_string(),
        "Row(id=3, score=2.5, name=\"c\")"
    );

    // A table sent to the server is stored under its path and can be fetched back.
    i.call("send", vec![t, Value::str(url.clone()), Value::str("copy")])
        .unwrap();
    assert_eq!(
        store.0.lock().unwrap()["copy"]
            .iter()
            .map(|b| b.num_rows())
            .sum::<usize>(),
        3
    );
    let back = i
        .call("fetch", vec![Value::str(url.clone()), Value::str("copy")])
        .unwrap();
    assert!(matches!(
        i.call("count", vec![back]).unwrap(),
        Value::Int(3)
    ));

    let e = i
        .call("fetch", vec![Value::str(url), Value::str("missing")])
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(e.contains("missing"), "{e}");
}

#[test]
fn flight_reports_bad_addresses() {
    let i = load();
    let e = i
        .call("fetch", vec![Value::str("localhost:1"), Value::str("t")])
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(e.contains("grpc://host:port"), "{e}");
    let e = i
        .call(
            "fetch",
            vec![Value::str("grpc://127.0.0.1:1"), Value::str("t")],
        )
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(e.contains("cannot connect"), "{e}");
}
