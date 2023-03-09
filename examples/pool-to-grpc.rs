use futures::StreamExt;
use scatter_gather_core::{
    middleware_specs::{
        ServerConfig, Interceptor,
    },
    pool::{
        Pool,
        PoolConfig,
        PoolConnectionLimits, 
    },
    connection::ConnectionHandlerOutEvent
};
use scatter_gather_websockets::WebSocketsMiddleware;
use scatter_gather_grpc::{
    GrpcMiddleware,
    schema_specific::{
        self, 
        OrderBook, 
        orderbook::{
            Summary, 
            Level
        }
    }
};
use scatter_gather::source_specs::{
    Depth,
    Level as DepthLevel,
    Interceptors,
    binance::BinanceDepthInterceptor,
    bitstamp::BitstampDepthInterceptor,
};
use std::{
    pin::Pin,
    any::type_name,
    task::Context,
    task::Poll
};
use tokio::runtime::Runtime;

fn type_of<T>(_: T) -> &'static str {
    type_name::<T>()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {

    let binance_interceptor = BinanceDepthInterceptor::new();
    let bitstamp_interceptor = BitstampDepthInterceptor::new();

    let config_binance: ServerConfig = ServerConfig {
        url : String::from("wss://stream.binance.com:9443/ws/ethbtc@depth@100ms"),
        prefix: String::from("wss://"),
        init_handle: None,
        // handler: binance_interceptor
    };
    let config_bitstamp: ServerConfig = ServerConfig { 
        url: String::from("wss://ws.bitstamp.net"), 
        prefix: String::from("wss://"), 
        init_handle: Some(r#"{"event": "bts:subscribe","data":{"channel": "diff_order_book_ethbtc"}}"#.to_string()),
        // handler: bitstamp_interceptor
    };

    let binance = WebSocketsMiddleware::new(config_binance).await ;
    let bitstamp = WebSocketsMiddleware::new(config_bitstamp).await;

    let binance_intercepted = 
        binance
            .read
            .map(|result| result.unwrap().into_text().unwrap())
            .map(|text| Interceptors::Binance(BinanceDepthInterceptor::intercept(text)) );
    let bitstamp_intercepted =
        bitstamp
            .read
            .map(|result| result.unwrap().into_text().unwrap())
            .map(|text| Interceptors::Bitstamp(BitstampDepthInterceptor::intercept(text)) );

    let grpc_config: ServerConfig = ServerConfig { 
        url: String::from("[::1]:54001"), 
        prefix: String::from("http://"), 
        init_handle: None, 
        // handler: Interceptors::Depth
    };


    let grpc = GrpcMiddleware::new(grpc_config);

    let pool_config1 = PoolConfig {
        task_event_buffer_size: 1
    };

    let pool_config2 = PoolConfig {
        task_event_buffer_size: 1
    };

    let mut ws_pool: Pool<WebSocketsMiddleware, Interceptors> = Pool::new(0_usize, pool_config1, PoolConnectionLimits::default()); 
    let mut grpc_pool: Pool<GrpcMiddleware,Interceptors> = Pool::new(1_usize, pool_config2, PoolConnectionLimits::default());


    grpc_pool.inject_connection(grpc);


    ws_pool.collect_streams(Box::pin(binance_intercepted));
    ws_pool.collect_streams(Box::pin(bitstamp_intercepted));

    // grpc_pool.intercept_stream().await;

    match grpc_pool.connect().await {
        Poll::Ready(conn) => {
            println!("Buffering.");
            let mut conn_lock = conn.lock().await;
            while let Some(intercepted) = ws_pool.next().await {
                let data = match intercepted {
                    Interceptors::Binance(point) => {
                        println!("{:?}",point.get_bids());
                    }
                    Interceptors::Bitstamp(point) => {
                        println!("{:?}",point.get_bids());
                    }
                    Depth => {
                        ()
                    }
                };
                conn_lock
                    .write
                    .send(ConnectionHandlerOutEvent::ConnectionEvent(Ok(Summary::default())))
                    .await?;
                conn_lock.client_buf().await.expect("Unable to buffer gRPC channel.");
            }

        }
        Poll::Pending => {}
    };
    println!("Connected ?");
    // grpc_pool.connect().await;

    // grpc_pool.intercept_stream().await;
    // let pool_config2 = PoolConfig {
    //     task_event_buffer_size: 1
    // };
    // let mut grpc_pool: Pool<GrpcMiddleware<Interceptors>, Interceptors> = Pool::new(1_usize, pool_config2, PoolConnectionLimits::default());

    // grpc_pool.collect_streams(Box::pin(ws_pool.local_streams));


    Ok(())
    
}