use anyhow::{anyhow,Result};
use std::fs::{File, set_permissions, Permissions};
use std::io::{Read, Write};
use std::env;
use std::collections::{HashMap};
use std::path::PathBuf;
use serde_derive::{Serialize,Deserialize};
use clap::{Parser, Subcommand};
use std::process::Command;
use std::process::Stdio;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use url::Url;
use prettytable::{Table, Row, Cell, format};
use std::os::unix::fs::PermissionsExt;
 
#[macro_use] extern crate prettytable;

const CACHE: &str = ".remote_jupyter_sessions";

pub enum ConnectionStatus {
    Connected,
    Disconnected
}

impl ConnectionStatus {
    pub fn msg(&self) -> String {
        match self {
            ConnectionStatus::Connected => "connected".to_string(),
            ConnectionStatus::Disconnected => "disconnected".to_string()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub host: String,
    pub port: u16,
    pub link: String,
    pub pid: Option<u32>,
    pub token: String
}

pub struct UrlParts {
    port: u16,
    token: String
}

impl UrlParts {
    pub fn parse(link: &str) -> Result<Self> {
        let parsed_url = Url::parse(link).expect("Failed to parse Jupyter URL.");
        let port = match parsed_url.port() {
            Some(port) => port,
            None => { 
                return Err(anyhow!("Incorrect Jupyter link format: no port in URL."))
            }
        };

        // get the token from the URL's parameters.
        let mut token: Option<String> = None;
        let query_pairs = parsed_url.query_pairs();
        for (key, value) in query_pairs {
            if key == "token" {
                token = Some(value.to_string());
                break;
            }
        }

        let token = match token {
            None => {
                return Err(anyhow!("Incorrect Jupyter link format: cannot determine authentication token."));
            },
            Some(value) => value
        };
        Ok(UrlParts {
            port,
            token
        })
    }
}

fn is_pid_running(pid: Pid) -> bool {
    kill(pid, Some(Signal::SIGCHLD)).is_ok()
}

fn format_key(conn: &Connection) -> String {
    format!("{}:{}", conn.host, conn.port)
}

impl Connection {
    pub fn new(link: &str, host: &str) -> Result<Connection> {
        let url_parts = UrlParts::parse(link)?;
        // Initiate the connection and return the struct.
        let pid = Connection::new_connection(host, url_parts.port)?;
        Ok(Connection { 
            host: host.to_string(),
            port: url_parts.port,
            link: link.to_string(),
            pid: Some(pid),
            token: url_parts.token
        })
    }
    
    pub fn get_pid(&self) -> Option<u32> {
        match self.status() {
            ConnectionStatus::Disconnected => None,
            ConnectionStatus::Connected => {
                self.pid
            }
        }
    }

    pub fn status(&self) -> ConnectionStatus {
        let pid = match self.pid {
            None => return ConnectionStatus::Disconnected,
            Some(p) => Pid::from_raw(p as i32)
        };
        let pid_alive = is_pid_running(pid);
        match pid_alive {
            true => ConnectionStatus::Connected,
            false => ConnectionStatus::Disconnected
        }
    }

    pub fn is_alive(&self) -> bool {
        let pid = match self.pid {
            None => return false,
            Some(p) => Pid::from_raw(p as i32)
        };
        is_pid_running(pid)
   }

    pub fn new_connection(host: &str, port: u16) -> Result<u32> {
        let ssh_command = format!(
            "ssh -Y -N -L localhost:{port}:localhost:{port} {host}",
            port = port,
            host = host);

        let child = Command::new("sh")
            .arg("-c")
            .arg(ssh_command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(child.id())
    }

    pub fn key(&self) -> String {
        format_key(self)
    }

    fn kill_connection(&mut self) -> Result<()> {
        match self.pid {
            None => {
                println!("Connection has already closed.");
            },
            Some(p) => match self.status() {
                ConnectionStatus::Connected => {
                    let pid = Pid::from_raw(p as i32);
                    // Send the SIGTERM signal
                    kill(pid, Signal::SIGTERM)?;
                    println!("Disconnected session {}:{} (Process ID={}).", self.host, self.port, p);
                },
                ConnectionStatus::Disconnected => {
                    println!("Connection has already closed.");
                }
            }
        }
        self.pid = None;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ConnectionCache {
    connections: HashMap<String,Connection>
}

impl ConnectionCache {
    fn cache_path() -> Result<PathBuf> {
        let home_dir = env::var("HOME")?;
        let path = PathBuf::from(home_dir).join(CACHE);
        Ok(path)
    }

    pub fn new() -> Self {
        ConnectionCache {
            connections: HashMap::new()
        }
    }

    fn load(&mut self) -> Result<()> {
        let cache_path = ConnectionCache::cache_path()?;
        // if we try to load the file and it doesn't exist, 
        // just create an empty cache.
        if !cache_path.exists() {
            self.connections = HashMap::new();
            self.save()?;
            return Ok(())
        }

        let mut file = File::open(cache_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let cache: HashMap<String,Connection> = if contents.trim().is_empty() {
            // a corner case: cache file exists but is empty. Handle same way
            // as if the file does not exist.
            self.connections = HashMap::new();
            self.save()?;
            return Ok(())
        } else {
            serde_yaml::from_str(&contents)?
        };

        self.connections = cache;
        Ok(())
    }

    pub fn list(&self) -> Result<()> {
        if self.connections.is_empty() {
            println!("No active remote Jupyter sessions.");
            return Ok(());
        }
        let mut table = Table::new();
        table.set_titles(row!["Key (host:port)", "Process ID", "Status", "Link"]);
        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
        for (key, conn) in self.connections.iter() {
            let status = conn.status();
            let status_cell = match status {
                ConnectionStatus::Connected => {
                    Cell::new(&status.msg()).style_spec("bFg")
                }, 
                ConnectionStatus::Disconnected => {
                    Cell::new(&status.msg()).style_spec("bFr")
                }
            };
            //table.add_row(row![key, conn.pid, conn.host, conn.port, status, conn.link]);
            let pid = conn.get_pid().map_or(" ".to_string(), |p| p.to_string());
            table.add_row(Row::new(vec![Cell::new(key), 
                                   Cell::new(&pid.to_string()),
                                   status_cell,
                                   Cell::new(&conn.link.to_string()), 
            ]));
        }
        table.printstd();
        Ok(())
    }

    pub fn reconnect(&mut self, key: &str) -> Result<()> {
        let conn = self.remove_connection(key)?;
        if conn.is_alive() {
            return Ok(())
        }
        let new_conn = Connection::new(&conn.link, &conn.host)?;
        self.connections.insert(key.to_string(), new_conn);
        println!("Reconnected session {}.", key);
        Ok(())
    }

    pub fn reconnect_all(&mut self) -> Result<()> {
        let keys: Vec<String> = self.connections.keys().cloned().collect();
        for key in keys {
            self.reconnect(&key)?;
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let serialized_cache = serde_yaml::to_string(&self.connections)
            .map_err(|err| anyhow::anyhow!("Failed to serialize data manifest: {}", err))?;

        // Create the file
        let cache_path = ConnectionCache::cache_path()?;
        let mut file = File::create(&cache_path)
            .map_err(|err| anyhow::anyhow!("Failed to open file '{:?}': {}", cache_path, err))?;

        // set the permissions such that only user has read/write
        let permissions = Permissions::from_mode(0o600);
          set_permissions(&cache_path, permissions)
        .map_err(|err| anyhow::anyhow!("Failed to set file permissions: {}", err))?;

        // Write the serialized data to the file
        write!(file, "{}", serialized_cache)
            .map_err(|err| anyhow::anyhow!("Failed to write the remote Jupyter cache: {}", err))?;
        Ok(())
    }

    pub fn new_connection(&mut self, link: &str, host: &str) -> Result<()> {
        let url_parts = UrlParts::parse(link)?;
        let key = format!("{}:{}", host, url_parts.port);
        if self.connections.contains_key(&key) {
            return Err(anyhow!("A remote Jupyter session with key '{}' is already registered.\n\
                               If you'd like to reconnect, use 'sdf rc'.", &key));
        }
        let connection = Connection::new(link, host)?;
        self.connections.insert(connection.key(), connection);
        println!("Created new session {}:{}.", host, url_parts.port);
        Ok(())
    }
    pub fn drop_connection(&mut self, key: &str) -> Result<()> {
        let mut conn = match self.connections.remove(key) {
            None => {
                return Err(anyhow!("Could not find a remote Jupyter session with key '{}'.", &key));
            }
            Some(conn) => conn
        };
        conn.kill_connection()
    }
    pub fn remove_connection(&mut self, key: &str) -> Result<Connection> {
        match self.connections.remove(key) {
            None => Err(anyhow!("Could not find a remote Jupyter session with key '{}'.", &key)),
            Some(conn) => Ok(conn)
        }
    }
    pub fn drop_all_connections(&mut self) -> Result<()> {
        let keys: Vec<String> = self.connections.keys().cloned().collect();
        for key in keys {
            self.drop_connection(&key)?;
        }
        Ok(())
    }
    pub fn disconnect(&mut self, key: &str) -> Result<()> {
        let conn = match self.connections.get_mut(key) {
            None => Err(anyhow!("Could not find a remote Jupyter session with key '{}'.", &key)),
            Some(conn) => Ok(conn)
        }?;
        conn.kill_connection()?;
        Ok(())
    }
    pub fn disconnect_all(&mut self) -> Result<()> {
        let keys: Vec<String> = self.connections.keys().cloned().collect();
        for key in keys {
            self.disconnect(&key)?;
        }
        Ok(())
    }
}

#[derive(Parser)]
#[clap(name = "rjy")]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a data file to the manifest.
    New {
        #[arg(required = true)]
        link: String,
        #[arg(required = true)]
        host: String
    },
    List {
    },
    Drop {
        key: Option<String>,
        #[arg(long)]
        all: bool
    },
    Rc {
        key: Option<String>
    },
    Dc {
        key: Option<String>
    }
}

fn main() {
    match run() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {:?}", e);
            std::process::exit(1);
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Some(Commands::New { link, host }) => {
            let mut sessions = ConnectionCache::new();
            sessions.load()?;
            sessions.new_connection(link, host)?;
            sessions.save()
        },
        Some(Commands::List { }) => {
            let mut sessions = ConnectionCache::new();
            sessions.load()?;
            sessions.list()?;
            Ok(())
        },
        Some(Commands::Rc { key }) => {
            let mut sessions = ConnectionCache::new();
            sessions.load()?;
            match key {
                None => sessions.reconnect_all()?,
                Some(k) => sessions.reconnect(k)?
            }
            sessions.save()
        },
        Some(Commands::Dc { key }) => {
            let mut sessions = ConnectionCache::new();
            sessions.load()?;
            match key {
                None => sessions.disconnect_all()?,
                Some(k) => sessions.disconnect(k)?
            }
            Ok(())
        },
        Some(Commands::Drop { key, all }) => {
            let mut sessions = ConnectionCache::new();
            sessions.load()?;
            if *all {
                sessions.drop_all_connections()?;
            } else {
                match key {
                    None => {
                        return Err(anyhow!("Specify either a key or --all, not both."));
                    },
                    Some(k) => {
                        sessions.drop_connection(k)?;
                    }
                }
            }
            sessions.save()
        },
        None => {
            std::process::exit(1);
        }
    }
}

