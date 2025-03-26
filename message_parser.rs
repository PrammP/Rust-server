use std::{collections::HashMap, io::{BufReader, BufRead,  Read}};
use std::net::TcpStream;

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

pub struct MessageParser;

impl MessageParser {
    pub fn parse_request(stream: &mut TcpStream, max_body_size: usize) -> Result<Request, String> {
        let mut buf_reader = BufReader::new(stream);
        let mut request_line = String::new();
        
        buf_reader.read_line(&mut request_line).map_err(|e| e.to_string())?;
        let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
        
        if parts.len() < 3 {
            return Err("Invalid request line".to_string());
        }

        let method = parts[0].to_string();
        let path = parts[1].to_string();

        let mut headers = HashMap::new();
        let mut content_length = 0;
        for line in buf_reader.by_ref().lines() {
            let line = line.map_err(|e| e.to_string())?;
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

        if content_length > max_body_size {
            return Err("Payload Too Large".to_string());
        }

        let mut body = String::new();
        if content_length > 0 {
            let mut buffer = vec![0; content_length];
            buf_reader.read_exact(&mut buffer).map_err(|e| e.to_string())?;
            body = String::from_utf8_lossy(&buffer).to_string();
        }

        Ok(Request {
            method,
            path,
            headers,
            body,
        })
    }
}
