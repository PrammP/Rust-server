
use server::ThreadPool;
use std::{
    fs,
    net::{TcpListener, TcpStream},
    io::{self, prelude::*, BufReader, Read, Write, ErrorKind},
    thread,
    time::Duration,
    collections::HashMap,
    process::Command,
    sync::{mpsc, Arc, Mutex},
};

#[derive(Clone)]
struct ServerConfig {
    hostname: String,
    port: u16,
    root_dir: String,
    error_pages: HashMap<u16, String>,
    max_body_size: usize,
    allowed_methods: Vec<String>,
    default_file: String,
}

fn handle_connection(mut stream: TcpStream, config: &ServerConfig) -> io::Result<()> {
    let mut buf_reader = BufReader::new(&stream);
    let mut request_line = String::new();

    buf_reader.read_line(&mut request_line)?;

    let mut headers = HashMap::new();
    let mut body = String::new();
    let mut content_length = 0;

    for line in buf_reader.by_ref().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        if line.is_empty() {
            break;
        }

        if let Some((key, value)) = line.split_once(": ") {
            headers.insert(key.to_string(), value.to_string());
            if key.eq_ignore_ascii_case("Content-Length") {
                content_length = value.parse::<usize>().unwrap_or(0);
            }
        }
    }

    if content_length > config.max_body_size {
        return send_error_response(&mut stream, 413, config);
    }

    if content_length > 0 {
        let mut buffer = vec![0; content_length];
        buf_reader.read_exact(&mut buffer)?;
        body = String::from_utf8_lossy(&buffer).to_string();
    }

    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 3 {
        return send_error_response(&mut stream, 400, config);
    }

    let method = parts[0];
    let path = parts[1];
    let host = headers.get("Host").cloned().unwrap_or_default();

    if !host.starts_with(&config.hostname) {
        return send_error_response(&mut stream, 404, config);
    }

    if !config.allowed_methods.contains(&method.to_string()) {
        return send_error_response(&mut stream, 405, config);
    }

    let (status_line, response_body) = match method {
        "GET" => handle_get(path, config),
        "POST" => handle_post(path, &body, config),
        "DELETE" => handle_delete(path, config),
        _ => (
            "HTTP/1.1 405 Method Not Allowed".to_string(), 
            "Method not allowed".to_string()
        ),
    };

    let contents = fs::read_to_string(path).unwrap_or_else(|_| response_body);
    let response = format!(
        "{status_line}\r\nContent-Length: {}\r\n\r\n{}", 
        contents.len(), 
        contents
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()?;

    Ok(())
}

fn send_error_response(
    stream: &mut TcpStream, 
    status_code: u16, 
    config: &ServerConfig
) -> io::Result<()> {
    let error_page = config.error_pages.get(&status_code)
        .cloned()
        .unwrap_or_else(|| format!("Error {}", status_code));

    let response = format!(
        "HTTP/1.1 {status_code} {}\r\nContent-Length: {}\r\n\r\n{}", 
        match status_code {
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            413 => "Payload Too Large",
            _ => "Internal Server Error"
        },
        error_page.len(),
        error_page
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()?;

    Ok(())
}

fn handle_get(_path: &str, config: &ServerConfig) -> (String, String) {
    if _path.starts_with("/cgi-bin/") {
        let script_path = format!("{}{}", config.root_dir, _path);
        match Command::new("python3").arg(&script_path).output() {
            Ok(output) => {
                let body = String::from_utf8_lossy(&output.stdout).to_string();
                return ("HTTP/1.1 200 OK\r\nContent-Type: text/plain".to_string(), body);
            }
            Err(_) => return ("HTTP/1.1 500 Internal Server Error".to_string(), "CGI script failed".to_string()),
        }
    }

    let mut filename = format!("{}/index.html", config.root_dir);
    let mut content_type = "text/html";

    if _path != "/" {
        filename = format!("{}{}", config.root_dir, _path);
        content_type = match _path.split('.').last() {
            Some("css") => "text/css",
            Some("js") => "application/javascript",
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            _ => "text/plain",
        };
    }

    match fs::read(&filename) {
        Ok(contents) => {
            let response_body = String::from_utf8_lossy(&contents).to_string();
            (format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}"), response_body)
        }
        Err(_) => {
            let error_page = format!("{}/404.html", config.root_dir);
            let response_body = fs::read_to_string(&error_page).unwrap_or_default();
            ("HTTP/1.1 404 Not Found".to_string(), response_body)
        }
    }
}

fn handle_post(_path: &str, body: &str, config: &ServerConfig) -> (String, String) {
    if body.len() > config.max_body_size {
        return ("HTTP/1.1 413 Payload Too Large".to_string(), "Request body too large".to_string());
    }
    println!("Received POST data: {}", body);
    ("HTTP/1.1 200 OK".to_string(), format!("Received data: {}", body))
}

fn handle_delete(_path: &str, _config: &ServerConfig) -> (String, String) {
    ("HTTP/1.1 200 OK".to_string(), "Delete request received".to_string())
}

fn main() -> io::Result<()> {
    let configs = vec![
        ServerConfig {
            hostname: "localhost".to_string(),
            port: 8080,
            root_dir: "src".to_string(),
            error_pages: [
                (404, "404.html".to_string()),
                (500, "500.html".to_string()),
                (405, "405.html".to_string()),
            ].iter().cloned().collect(),
            max_body_size: 1024 * 1024, 
            allowed_methods: vec!["GET".to_string(), "POST".to_string(), "DELETE".to_string()],
            default_file: "index.html".to_string(),
        },
    ];

    let mut listeners = Vec::new();
    for config in &configs {
        let listener = TcpListener::bind(format!("{}:{}", config.hostname, config.port))?;
        listener.set_nonblocking(true)?;
        listeners.push((listener, config.clone()));
    }

    let pool = ThreadPool::new(4)?;

    loop {
        for (listener, config) in &listeners {
            if let Ok((stream, _)) = listener.accept() {
                let config = config.clone();
                pool.execute(move || {
                    if let Err(e) = handle_connection(stream, &config) {
                        eprintln!("Error handling connection: {}", e);
                    }
                });
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
}