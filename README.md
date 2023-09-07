# Remote Jupyter Session Management Tool

![Crates.io](https://img.shields.io/crates/v/remote_jupyter) ![Crates.io (latest)](https://img.shields.io/crates/dv/remote_jupyter)


![a screenshot of the rjy command line tool](https://github.com/vsbuffalo/remote_jupyter/blob/main/screenshot.png?raw=true)

`rjy` Rust command-line tool that manages SSH tunneling for working with
multiple Jupyter notebooks and lab instances over SSH. The command-line tool
spawns a background SSH process and manages session information via a cache in
`~/.remote_jupyter_sessions`.

First, create a remote Jupyter session on a server with,

    $ jupyter lab --no-browser --port=8904

where 8904 is a random high port number.

Then, copy the link it provides and use it with `rjy new <link>` *and your 
remote server hostname* to register this session with your local computer,

    $ rjy new http://localhost:8904/lab?token=b1fc6[...]b7a40 remote
    Created new session ponderosa:8906.

You could use an IP address too, but I **strongly** recommend if you interact
with servers a lot over SSH, you add them to your `~/.ssh/config` file (see
[this page](https://linuxhandbook.com/ssh-config-file/), for example) and refer
to them by their hostnames. You also should use `ssh-add`, so that you won't be
prompted for a password each time. These may seem like frustrating extra steps,
but both of these tips will greatly simplify working with remote servers a lot!

Then, we can see this Jupyter session is "registered" and the SSH tunneling
is with `rjy list`:

    $ rjy list
     Key (host:port) | Process ID | Status    | Link                              
    -----------------+------------+-----------+-----------------------------------------------
     ponderosa:8906  | 68190      | connected | http://localhost:8906/lab?token=5e2f[...]8467
     sesame:8906     | 67087      | connected | http://127.0.0.1:8906/lab?token=3aa1[...]bee1
    
Most good terminals will allow you to directly click this link (e.g.
in iTerm2 on Mac, if you hold `⌘` and hover over a link, it will
become clickable).

We can disconnect a session with `rjy dc <key>`, where the key is that in the
list output. If no key is specified, all sessions are disconnected.

    $ rjy dc remote:8904
    Disconnected 'sesame:8906' (Process ID=67087).

Now we can see it's disconnected:

    $ rjy list
     Key (host:port) | Process ID | Status       | Link                              
    -----------------+------------+--------------+-----------------------------------------------
     ponderosa:8906  | 68190      | connected    | http://localhost:8906/lab?token=5e2f[...]8467
     sesame:8906     |            | disconnected | http://127.0.0.1:8906/lab?token=3aa1[...]bee1
  
    
We can reconnect with `rjy rc`. Without a key, everything registered is 
reconnected. With a key, only that session is.

    $ rjy rc remote:8904
    Reconnected session sesame:8906.

Now if we check,

    $ rjy list
     Key (host:port) | Process ID | Status    | Link
    -----------------+------------+-----------+----------------------------------------------------------------------------------
     sesame:8906     | 69233      | connected | http://127.0.0.1:8906/lab?token=3aa1[...]bee1
     ponderosa:8906  | 68883      | connected | http://localhost:8906/lab?token=5e2f[...]8467
    
it's reconnected as expected. Finally, to drop a session from the registered
cache (kept in `~/.remote_jupyter_sessions`), use `rjy drop <key>`:

    $ rjy drop ponderosa:8906
    Disconnected 'ponderosa:8906' (Process ID=68883).

You can also drop all connections with `rjy drop --all`. See the built-in
instructions with `rjy --help` for more information.

## Security 

This stores the token Jupyter creates in `~/.remote_jupyter_sessions`, and sets
the permissions so only the owner has read/write permissions. This is as secure
as having the authentication token in your shell history, but caution is still
warranted. Do not use on untrusted systems. 

## Install
    
    $ cargo install remote_jupyter

