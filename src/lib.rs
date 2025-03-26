use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::io::ErrorKind;


pub struct ThreadPool {
    _workers: Vec<Worker>, 
    sender: mpsc::Sender<Job>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    pub fn new(size: usize) -> Result<ThreadPool, std::io::Error> {
        if size == 0 {
            return Err(std::io::Error::new(
                ErrorKind::InvalidInput, 
                "Thread pool size must be greater than zero"
            ));
        }

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        Ok(ThreadPool { 
            _workers: workers, 
            sender 
        })
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        if let Err(_) = self.sender.send(job) {
            eprintln!("Failed to send job to thread pool");
        }
    }
}

struct Worker {
    _id: usize, 
    _thread: thread::JoinHandle<()>, 
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
        let thread = thread::spawn(move || {
            loop {
                
                match receiver.lock().unwrap().recv() {
                    Ok(job) => {
                        println!("Worker {} got a job; executing.", id);
                        job();
                    }
                    Err(_) => {
                        println!("Worker {} disconnected; shutting down.", id);
                        break;
                    }
                }
            }
        });

        Worker { 
            _id: id, 
            _thread: thread 
        }
    }
}