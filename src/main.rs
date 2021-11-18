use actix::{Actor, ActorContext, AsyncContext, Handler, Message, Recipient, StreamHandler};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use lazy_static::lazy_static;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{io, mem, thread};
use websocket::ClientBuilder;

// If set to 10, the interval func runs properly and shuts down the severed connection.
// If set to 10000, the interval func doesn't run (no Pinging/Stopping messages in stdout).
const AMOUNT: usize = 10000;

lazy_static! {
    static ref FIREHOSES: Arc<Mutex<HashSet<Recipient<Water>>>> =
        Arc::new(Mutex::new(HashSet::new()));
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    let mut counter = 0;
    thread::spawn(move || loop {
        println!("{}, {:?}", counter, connection_counts());
        counter += 1;

        thread::sleep(Duration::from_millis(1000));

        if counter == 21 {
            println!("No more connections will be leaked to avoid crashing. Monitor the existing connections to see if they ever close.")
        } else if counter < 21 {
            let client = ClientBuilder::new("http://localhost:1080/firehose")
                .unwrap()
                .add_protocol("rust-websocket")
                .connect_insecure()
                .unwrap();

            mem::forget(client);
        }
    });

    thread::spawn(|| loop {
        thread::sleep(Duration::from_millis(100));
        let mut err_count = 0;
        let mut total = 0;
        for firehose in FIREHOSES.lock().unwrap().iter() {
            if firehose.do_send(Water).is_err() {
                err_count += 1;
            }
            total += 1;
        }

        if err_count > 0 {
            println!("{} out of {} do_send(Water) failed.", err_count, total);
        }
    });

    let app = move || App::new().service(web::resource("/firehose").route(web::get().to(index)));

    HttpServer::new(app)
        .keep_alive(1)
        .bind("0.0.0.0:1080")?
        .run()
        .await
}

struct Firehose {
    last_activity: Instant,
}

impl Actor for Firehose {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_secs(3), |act, ctx| {
            if act.last_activity.elapsed() > Duration::from_secs(10) {
                ctx.close(None);
                ctx.stop();
                println!("Stopping WS...");
            } else {
                ctx.ping(b"");
                println!("Pinging WS...");
            }
        });

        println!("A firehose is starting");
        FIREHOSES.lock().unwrap().insert(ctx.address().recipient());
        println!("A firehose started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        println!("A firehose is stopping");
        FIREHOSES.lock().unwrap().remove(&ctx.address().recipient());
        println!("A firehose stopped");
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Firehose {
    fn handle(&mut self, _msg: Result<ws::Message, ws::ProtocolError>, _ctx: &mut Self::Context) {
        self.last_activity = Instant::now();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct Water;

impl Handler<Water> for Firehose {
    type Result = ();

    fn handle(&mut self, _: Water, ctx: &mut Self::Context) -> Self::Result {
        // Enough to fill the network interface on my computer.
        for _ in 0..AMOUNT {
            ctx.text("drip drip drip drip drip drip drip drip drip drip drip drip drip drip drip");
        }
    }
}

async fn index(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, actix_web::Error> {
    ws::start(
        Firehose {
            last_activity: Instant::now(),
        },
        &req,
        stream,
    )
}

/// Returns the count of each connection state on the process, including both client and server
/// sockets.
fn connection_counts() -> BTreeMap<String, usize> {
    let mut ret = BTreeMap::new();

    let output = Command::new("netstat")
        .arg("-natp")
        .output()
        .expect("net-tools must be installed");
    for s in std::str::from_utf8(&output.stdout)
        .unwrap()
        .split('\n')
        .filter(|s| {
            s.contains("tcp-leak") || s.contains("target/debug") || s.contains("target/release")
        })
    {
        let mut key = None;
        for potential in [
            "LISTEN",
            "SYN_SENT",
            "SYN_RECEIVED",
            "ESTABLISHED",
            "FIN_WAIT_1",
            "FIN_WAIT_2",
            "CLOSE_WAIT",
            "CLOSING",
            "LAST_ACK",
            "TIME_WAIT",
            "CLOSED",
        ] {
            if s.contains(potential) {
                key = Some(potential);
                break;
            }
        }

        if let Some(key) = key {
            *ret.entry(key.to_owned()).or_insert(0) += 1;
        }
    }

    ret
}
