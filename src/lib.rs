mod error;
mod opt;

use std::sync::Arc;
use std::time::Duration;

use error::err_map;
use napi::bindgen_prelude::*;
use napi::tokio::sync::RwLock;
use napi::tokio::sync::Semaphore;
use napi_derive::napi;

use opt::endpoint::Options;
use serde_json::from_value;
use serde_json::Value as JsValue;
use surrealdb::dbs::Session;
use surrealdb::kvs::Datastore;
use surrealdb::rpc::format::cbor;
use surrealdb::kvs::export::Config;

use surrealdb::rpc::{Data, RpcContext};
use surrealdb::sql::Value;
use uuid::Uuid;

#[napi]
pub struct SurrealdbNodeEngine(RwLock<Option<SurrealdbNodeEngineInner>>);

#[napi]
impl SurrealdbNodeEngine {
    #[napi]
    pub async fn execute(&self, data: Uint8Array) -> std::result::Result<Uint8Array, Error> {
        let in_data = cbor::req(data.to_vec()).map_err(err_map)?;

        let data =
			self.0
				.read()
				.await
				.as_ref()
				.ok_or_else(|| Error::from_reason("Use after free"))?
				.execute(in_data.version, in_data.method, in_data.params)
				.await
				.map_err(err_map)?;

		let value: Value = data.try_into().map_err(err_map)?;
		let out = cbor::res(value).map_err(err_map)?;
		Ok(out.as_slice().into())
    }

    // pub fn notifications(&self) -> std::result::Result<sys::ReadableStream, Error> {
    //     let stream = self.0.kvs.notifications().ok_or("Notifications not enabled")?;
    //
    //
    //     let response = stream.map(|notification| {
    //         let json = json!({
    // 			"id": notification.id,
    // 			"action": notification.action.to_string(),
    // 			"result": notification.result.into_json(),
    // 		});
    //         to_value(&json).map_err(Into::into)
    //     });
    //     Ok(ReadableStream::from_stream(response).into_raw())
    // }

    #[napi]
    pub async fn connect(
        endpoint: String,
        #[napi(ts_arg_type = "ConnectionOptions")] opts: Option<JsValue>,
    ) -> std::result::Result<SurrealdbNodeEngine, Error> {
        let endpoint = match &endpoint {
            s if s.starts_with("mem:") => "memory",
            s => s,
        };
        let kvs = Datastore::new(endpoint)
            .await
            .map_err(err_map)?
            .with_notifications();
        // let kvs = match opts.map(|o| o.try_into()) {
        //     None => kvs,
        //     Some(opts) => kvs
        //         .with_capabilities(
        //             opts.capabilities
        //                 .map_or(Ok(Default::default()), |a| a.try_into())?,
        //         )
        //         .with_transaction_timeout(
        //             opts.transaction_timeout
        //                 .map(|qt| Duration::from_secs(qt as u64)),
        //         )
        //         .with_query_timeout(opts.query_timeout.map(|qt| Duration::from_secs(qt as u64)))
        //         .with_strict_mode(opts.strict.map_or(Default::default(), |s| s)),
        // };

        // let kvs = if let Some(opts) = opts.map(from_value::<Option<Options>>) {
        //     kvs
        // } else {
        //     kvs
        // };

        let kvs = if let Some(o) = opts {
            let opts = from_value::<Options>(o)?;
            kvs.with_capabilities(
                opts.capabilities
                    .map_or(Ok(Default::default()), |a| a.try_into())?,
            )
            .with_transaction_timeout(
                opts.transaction_timeout
                    .map(|qt| Duration::from_secs(qt as u64)),
            )
            .with_query_timeout(opts.query_timeout.map(|qt| Duration::from_secs(qt as u64)))
            .with_strict_mode(opts.strict.map_or(Default::default(), |s| s))
        } else {
            kvs
        };

		// Check version or write if is not
		kvs.check_version().await.map_err(err_map)?;

        let session = Session::default().with_rt(true);

        let inner = SurrealdbNodeEngineInner::new(
            kvs,
            session,
        );

        Ok(SurrealdbNodeEngine(RwLock::new(Some(inner))))
    }

    #[napi]
    pub async fn free(&self) {
        let _inner_opt = self.0.write().await.take();
    }

    #[napi]
    pub fn version() -> std::result::Result<String, Error> {
        Ok(env!("SURREALDB_VERSION").into())
    }

	#[napi]
	pub async fn export(&self, config: Option<Uint8Array>) -> std::result::Result<String, Error> {
		let lock = self.0.read().await;
		let inner = lock.as_ref().unwrap();
		let session = inner.session();
		let (tx, rx) = channel::unbounded();

		match config {
			Some(config) => {
				let in_config = cbor::parse_value(config.to_vec()).map_err(err_map)?;
				let config = Config::try_from(&in_config).map_err(err_map)?;

				inner.kvs.export_with_config(&session, tx, config).await.map_err(err_map)?.await.map_err(err_map)?;
			}
			None => {
				inner.kvs.export(&session, tx).await.map_err(err_map)?.await.map_err(err_map)?;
			}
		};

		let mut buffer = Vec::new();
		while let Ok(item) = rx.try_recv() {
			buffer.push(item);
		}

		let result = String::from_utf8(buffer.concat().into()).map_err(err_map)?;

		Ok(result)
	}
}

struct SurrealdbNodeEngineInner {
    kvs: Datastore,
    session: std::sync::RwLock<Arc<Session>>,
	lock_semaphore: Arc<Semaphore>,
}

impl SurrealdbNodeEngineInner {
	fn new(kvs: Datastore, session: Session) -> Self {
		SurrealdbNodeEngineInner {
			kvs,
			session: std::sync::RwLock::new(Arc::new(session)),
			lock_semaphore: Arc::new(Semaphore::new(1))
		}
	}
}



impl RpcContext for SurrealdbNodeEngineInner {
    fn kvs(&self) -> &Datastore {
        &self.kvs
    }

    fn session(&self) -> Arc<Session> {
        self.session.read().unwrap().clone()
    }

    fn set_session(&self, session: Arc<Session>) {
		*self.session.write().unwrap() = session
    }

    fn version_data(&self) -> Data {
		Value::Strand(format!("surrealdb-{}", env!("SURREALDB_VERSION")).into()).into()
	}

	fn lock(&self) -> Arc<napi::tokio::sync::Semaphore> {
		self.lock_semaphore.clone()
	}

    const LQ_SUPPORT: bool = true;
    fn handle_live(&self, _lqid: &Uuid) -> impl std::future::Future<Output = ()> + Send {
        async { () }
    }
    fn handle_kill(&self, _lqid: &Uuid) -> impl std::future::Future<Output = ()> + Send {
        async { () }
    }
}

impl surrealdb::rpc::RpcProtocolV1 for SurrealdbNodeEngineInner {}
impl surrealdb::rpc::RpcProtocolV2 for SurrealdbNodeEngineInner {}
