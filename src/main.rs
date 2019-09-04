use habitat_core::os::process::windows_child::Child;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::{io::{self,
               BufRead,
               BufReader,
               Read},
          thread};
use mio_named_pipes::NamedPipe;
use mio::{Poll, Ready, Token, PollOpt, Events};
use std::os::windows::fs::*;
use std::os::windows::io::*;
use std::io::prelude::*;
use winapi::um::winbase;
use winapi::shared::winerror::ERROR_FILE_NOT_FOUND;
use retry::{delay, retry};

fn main() {
    println!("Welcome and enjoy your health checker POC!");
    println!("Type enter to check health.");

    let hook = HealthHook::new();

    loop {
        if io::stdin().read_line(&mut String::new()).is_ok() {
            let exit_status = hook.check_health();
            println!("server returned exit code {}", exit_status);
        }
    }
}

struct HealthHook {
    client: PipeClient,
}

impl HealthHook {
    pub fn new() -> Self {
        Self { client: PipeClient::new() }
    }

    pub fn check_health(&self) -> i32 {
        self.client.check_health()
    }
}

struct PipeClient {}

impl PipeClient {
    pub fn new() -> Self {
        PipeClient::init_server();
        Self {} 
    }

    pub fn check_health(&self) -> i32 {
        let mut events = Events::with_capacity(1024);
        println!("connecting to pipe...");

        let file = match retry(delay::Fixed::from_millis(100).take(50), || {
            Self::pipe_file()
        }) {
            Ok(f) => f,
            Err(err) => {
                if let Err(err) = PipeClient::pipe_file() {
                    if err.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) {
                        PipeClient::init_server();
                        retry(delay::Fixed::from_millis(100).take(50), || {
                            Self::pipe_file()
                        }).unwrap()
                    } else {
                        panic!("raw error: {:?}", err.raw_os_error());
                    }
                } else {
                    panic!("we failed {}", err);
                }
            }
        };

        let mut pipe = unsafe {
            NamedPipe::from_raw_handle(file.into_raw_handle())
        };
        println!("connected to pipe. Let the data flow!");
        let poll = Poll::new().unwrap();
        poll.register(&pipe,
                        Token(0),
                        Ready::all(),
                        PollOpt::edge()).unwrap();
        
        let mut exit_buf = [0; 4];
        poll.poll(&mut events, None).unwrap();
        pipe.write(&(1 as u32).to_ne_bytes()).unwrap();
        pipe.flush().unwrap();
        println!("sent health byte");

        let mut readable = false;
        while readable == false {
            poll.poll(&mut events, None).unwrap();
            for event in &events {
                if event.readiness().is_readable() {
                    readable = true;
                }
            }
        }   
        pipe.read(&mut exit_buf).unwrap();

        i32::from_ne_bytes(exit_buf)
    }

    fn pipe_file() -> io::Result<File> {
        let mut opts = OpenOptions::new();
        opts.read(true)
            .write(true)
            .custom_flags(winbase::FILE_FLAG_OVERLAPPED);
        opts.open("\\\\.\\pipe\\rust-ipc-bdd62f4b-2d3f-409c-a82d-5530be2ae8a1") 
    }

    fn init_server() {
        let args = vec!["-NonInteractive", "-file", "c:/dev/health/src/server.ps1"];
        let child = Child::spawn("C:/hab/pkgs/core/powershell/6.2.1/20190621130612/bin/pwsh.exe",
                        &args,
                        &HashMap::new(),
                        "matt",
                        None::<String>).unwrap();
        let out = child.stdout.unwrap();
        let err = child.stderr.unwrap();
        println!("spawned powershell pipe server");

        thread::Builder::new().name("health-server-out".to_string())
                                .spawn(move || pipe_stdout(out))
                                .ok();
        thread::Builder::new().name("health-server-err".to_string())
                                .spawn(move || pipe_stderr(err))
                                .ok();
    }
}

impl Drop for PipeClient {
    fn drop(&mut self) {
        let mut events = Events::with_capacity(1024);
        println!("connecting to pipe...");

        let file = retry(delay::Fixed::from_millis(100).take(50), || {
            Self::pipe_file()
        }).unwrap();
        let mut pipe = unsafe {
            NamedPipe::from_raw_handle(file.into_raw_handle())
        };
        println!("connected to pipe. Let the data flow!");
        let poll = Poll::new().unwrap();
        poll.register(&pipe,
                        Token(0),
                        Ready::all(),
                        PollOpt::edge()).unwrap();

        println!("Telling pipe server to quit...");
        poll.poll(&mut events, None).unwrap();
        pipe.write(&(0 as u32).to_ne_bytes()).unwrap();
        pipe.flush().unwrap();
    }
}

fn pipe_stdout<T>(out: T)
    where T: Read
{
    let mut reader = BufReader::new(out);
    let mut buffer = String::new();
    while reader.read_line(&mut buffer).unwrap() > 0 {
        let content = &buffer.trim_end_matches('\n');
        println!("{}", content);
        buffer.clear();
    }
}

fn pipe_stderr<T>(err: T)
    where T: Read
{
    let mut reader = BufReader::new(err);
    let mut buffer = String::new();
    while reader.read_line(&mut buffer).unwrap() > 0 {
        let content = &buffer.trim_end_matches('\n');
        eprintln!("{}", content);
        buffer.clear();
    }
}
