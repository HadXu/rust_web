use std::net::SocketAddr;

use hyper::{service::{make_service_fn, service_fn}, Body, Method, Request, Response, Server, StatusCode, Client, http};

use std::fs;
use futures::TryStreamExt as _;

use anyhow::{Error, Result};
use hyper::upgrade::Upgraded;
use tokio::net::TcpStream;

type HttpClient = Client<hyper::client::HttpConnector>;

#[tokio::main]
async fn main() {
    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 8100));

    let client = Client::builder()
        .http1_title_case_headers(true)
        .http1_preserve_header_case(true)
        .build_http();

    let make_service = make_service_fn(move |_| {
        let client = client.clone();
        async move { Ok::<_, Error>(service_fn(move |req| proxy(client.clone(), req))) }
    });


    let server = Server::bind(&addr)
        .http1_title_case_headers(true)
        .http1_preserve_header_case(true)
        .serve(make_service);

    println!("Listening on http://{}", addr);


    // A `Service` is needed for every connection, so this
    // creates one from our `hello_world` function.

    // let make_svc = make_service_fn(|_conn| async {
    //     // service_fn converts our function into a `Service`
    //     Ok::<_, Error>(service_fn(hello_world))
    // });

    // let server = Server::bind(&addr).serve(make_svc);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn proxy(client: HttpClient, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    println!("req: {:?}", req);

    if Method::CONNECT == req.method() {
        if let Some(addr) = host_addr(req.uri()) {
            tokio::task::spawn(async move {
                match hyper::upgrade::on(req).await {
                    Ok(upgraded) => {
                        if let Err(e) = tunnel(upgraded, addr).await {
                            eprintln!("server io error: {}", e);
                        };
                    }
                    Err(e) => eprintln!("upgrade error: {}", e),
                }
            });

            Ok(Response::new(Body::empty()))
        } else {
            eprintln!("CONNECT host is not socket addr: {:?}", req.uri());
            let mut resp = Response::new(Body::from("CONNECT must be to a socket address"));
            *resp.status_mut() = http::StatusCode::BAD_REQUEST;

            Ok(resp)
        }
    } else {
        client.request(req).await
    }
}

fn host_addr(uri: &http::Uri) -> Option<String> {
    uri.authority().and_then(|auth| Some(auth.to_string()))
}

async fn tunnel(mut upgraded: Upgraded, addr: String) -> std::io::Result<()> {
    // Connect to remote server
    let mut server = TcpStream::connect(addr).await?;

    // Proxying data
    let (from_client, from_server) =
        tokio::io::copy_bidirectional(&mut upgraded, &mut server).await?;

    // Print message when done
    println!(
        "client wrote {} bytes and received {} bytes",
        from_client, from_server
    );

    Ok(())
}


async fn hello_world(req: Request<Body>) -> Result<Response<Body>, Error> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            let content = fs::read_to_string("rust.md")?;

            *response.body_mut() = Body::from(content);
        }

        (&Method::GET, "/pdf") => {
            response
                .headers_mut()
                .insert("Content-type", "application/pdf".parse()?);
            let content = fs::read("xian.pdf")?;
            *response.status_mut() = StatusCode::OK;
            *response.body_mut() = Body::from(content);
        }

        (&Method::GET, "/html") => {
            response
                .headers_mut()
                .insert("Content-type", "text/html".parse()?);
            *response.body_mut() = Body::from("<h1>xxxx</h1>");
        }

        (&Method::GET, "/jpg") => {
            response.headers_mut().insert("Content-type", "".parse()?);
            let content = fs::read("rhesus.png")?;
            *response.status_mut() = StatusCode::OK;
            *response.body_mut() = Body::from(content);
        }

        (&Method::POST, "/echo") => *response.body_mut() = req.into_body(),

        (&Method::POST, "/echo/uppercase") => {
            let mapping = req.into_body().map_ok(|chunk| {
                chunk
                    .iter()
                    .map(|byte| byte.to_ascii_uppercase())
                    .collect::<Vec<u8>>()
            });

            *response.body_mut() = Body::wrap_stream(mapping);
        }

        (&Method::POST, "/echo/reverse") => {
            let full_body = hyper::body::to_bytes(req.into_body()).await?;
            let reversed = full_body.iter().rev().cloned().collect::<Vec<u8>>();

            *response.body_mut() = reversed.into();
        }

        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    }
    Ok(response)
}
