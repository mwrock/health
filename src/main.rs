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
use std::str;
use winapi::um::winbase;
use winapi::shared::winerror::ERROR_PIPE_BUSY;
use retry::{delay, retry};

fn main() {
    println!("Welcome and enjoy your health checker POC!");
    println!("Type 'h' to check health. Type 'q' to quit.");

    let mut input_buf = [0; 1];
    let mut client = HealthClient::new();

    loop {
        if io::stdin().read_exact(&mut input_buf).is_ok() {
            if !client.is_connected() {
                println!("lost server, creating new client.");
                client = HealthClient::new();
            }
            match str::from_utf8(&input_buf) {
                Ok("h") => {
                    let exit_status = client.check_health();
                    println!("server returned exit code {}", exit_status);
                },
                Ok("q") => {
                    client.quit();
                    break;
                },
                _ => {},
            }
        }
    }

    println!("Good Bye");
}

struct HealthClient {
    pipe: NamedPipe,
    poll: Poll,
    events: Events,
}

impl HealthClient {
    pub fn new() -> Self {
        let events = Events::with_capacity(1024);
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

        println!("connecting to pipe...");

        let file = retry(delay::Fixed::from_millis(100).take(50), || {
            Self::pipe_file()
        }).unwrap();
        let pipe = unsafe {
            NamedPipe::from_raw_handle(file.into_raw_handle())
        };
        println!("connected to pipe. Let the data flow!");
        let poll = Poll::new().unwrap();
        poll.register(&pipe,
                        Token(0),
                        Ready::all(),
                        PollOpt::edge()).unwrap();
        Self { 
            pipe,
            poll,
            events, 
        } 
    }

    pub fn check_health(&mut self) -> i32 {
        let mut exit_buf = [0; 4];
        self.poll.poll(&mut self.events, None).unwrap();
        self.pipe.write("c:/dev/health/src/hook.ps1\n".as_bytes()).unwrap();

        let mut readable = false;
        while readable == false {
            self.poll.poll(&mut self.events, None).unwrap();
            for event in &self.events {
                if event.readiness().is_readable() {
                    readable = true;
                }
            }
        }   
        self.pipe.read(&mut exit_buf).unwrap();

        i32::from_ne_bytes(exit_buf)
    }

    pub fn quit(&mut self) {
        println!("Telling pipe server to quit...");
        self.poll.poll(&mut self.events, None).unwrap();
        self.pipe.write("\n".as_bytes()).unwrap();
    }

    pub fn is_connected(&self) -> bool {
        if let Err(err) = HealthClient::pipe_file() {
            if err.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) {
                return true
            }
        }
        false
    }

    fn pipe_file() -> io::Result<File> {
        let mut opts = OpenOptions::new();
        opts.read(true)
            .write(true)
            .custom_flags(winbase::FILE_FLAG_OVERLAPPED);
        opts.open("\\\\.\\pipe\\rust-ipc-bdd62f4b-2d3f-409c-a82d-5530be2ae8a1") 
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
