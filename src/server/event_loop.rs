use crate::server::Server;
use libc::{kevent, kevent64_s, kqueue, EVFILT_READ, EVFILT_WRITE, EV_ADD, EV_DELETE, EV_ENABLE};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::time::Instant;

/// Create a new kqueue descriptor
pub fn create_kqueue() -> RawFd {
    let kq = unsafe { kqueue() };
    if kq == -1 {
        panic!(
            "Failed to create kqueue: {}",
            std::io::Error::last_os_error()
        );
    }
    kq
}

/// Register an event with the kqueue
pub fn register_event(kqueue: RawFd, fd: RawFd, filter: i16, flags: u16) -> std::io::Result<()> {
    let mut event = kevent64_s {
        ident: fd as u64,
        filter,
        flags,
        fflags: 0,
        data: 0,
        udata: 0,
        ext: [0, 0],
    };

    let result = unsafe {
        kevent(
            kqueue,
            &mut event as *mut _ as *const _,
            1,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    };

    if result == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Register a read event for a given fd
pub fn register_read(kqueue: RawFd, fd: RawFd) -> io::Result<()> {
    register_event(kqueue, fd, EVFILT_READ, EV_ADD | EV_ENABLE)
}

/// Register a write event for a given fd
pub fn register_write(kqueue: RawFd, fd: RawFd) -> io::Result<()> {
    register_event(kqueue, fd, EVFILT_WRITE, EV_ADD | EV_ENABLE)
}

/// Deregister a read event
pub fn deregister_read(kqueue: RawFd, fd: RawFd) -> io::Result<()> {
    register_event(kqueue, fd, EVFILT_READ, EV_DELETE)
}

/// Deregister a write event
pub fn deregister_write(kqueue: RawFd, fd: RawFd) -> io::Result<()> {
    register_event(kqueue, fd, EVFILT_WRITE, EV_DELETE)
}

/// Deregister both read and write events
pub fn deregister_all(kqueue: RawFd, fd: RawFd) -> io::Result<()> {
    let _ = deregister_read(kqueue, fd);
    let _ = deregister_write(kqueue, fd);
    Ok(())
}

/// Register all listening sockets for read events
pub fn register_listeners(server: &Server, kqueue: RawFd) {
    for socket in &server.sockets {
        let fd = socket.listener.as_raw_fd();
        if let Err(e) = register_read(kqueue, fd) {
            panic!("[!] Failed to register listener {}: {}", fd, e);
        }
    }
}

/// Process a single kqueue event
pub fn process_event(server: &mut Server, kqueue: RawFd, ev: &kevent64_s) {
    let fd = ev.ident as i32;

    // 1. Accept new connections if this is a listener socket
    if let Some(socket) = server.sockets.iter().find(|s| s.listener.as_raw_fd() == fd) {
        let new_clients = socket.try_accept();
        for client in new_clients {
            let cfd = client.stream.as_raw_fd();
            if let Err(e) = register_read(kqueue, cfd) {
                eprintln!("[!] Failed to register client {}: {}", cfd, e);
                continue;
            }
            server.clients.push(client);
        }
        return;
    }

    // 2. Handle existing client
    if let Some(pos) = server
        .clients
        .iter()
        .position(|c| c.stream.as_raw_fd() == fd)
    {
        let mut client = server.clients.swap_remove(pos);

        let keep = match ev.filter {
            EVFILT_READ => server.handle_client_read(&mut client),
            EVFILT_WRITE => server.handle_client_write(&mut client),
            _ => Ok(true), // Ignore unknown filters
        }
        .unwrap_or(false);

        if keep {
            if client.has_pending_write() {
                if !client.write_registered {
                    let _ = register_write(kqueue, fd);
                    client.write_registered = true;
                }
            } else if ev.filter == EVFILT_WRITE {
                // Only deregister if this was a write event and we know it's drained.
                let _ = deregister_write(kqueue, fd);
                client.write_registered = false;

                if client.should_close {
                    let _ = client.stream.shutdown(std::net::Shutdown::Both);
                    let _ = deregister_all(kqueue, fd);
                    return;
                }
            }
            server.clients.push(client);
        } else {
            // Deregister both read and write for closed client
            let _ = deregister_all(kqueue, fd);
        }
    }
}

/// Checks for clients that have been idle longer than server.client_timeout and closes them.
fn cleanup_idle_clients(server: &mut Server) {
    let now = Instant::now();
    server.clients.retain_mut(|client| {
        if let Some(last_req) = client.request_at {
            if now.duration_since(last_req) > server.client_timeout {
                eprintln!(
                    "Closing client due to request timeout: {}",
                    client.peer_addr
                );

                // Close the connection
                let _ = client.stream.shutdown(std::net::Shutdown::Both);
                return false;
            }
        }
        true
    });
}

/// The main server event loop
pub fn run_loop(server: &mut Server) {
    let kqueue = create_kqueue();
    register_listeners(server, kqueue);

    let mut events = vec![
        kevent64_s {
            ident: 0,
            filter: 0,
            flags: 0,
            fflags: 0,
            data: 0,
            udata: 0,
            ext: [0, 0],
        };
        1024
    ];

    loop {
        // Timeout for kevent: 100ms
        let timeout = libc::timespec {
            tv_sec: 0,
            tv_nsec: 100_000_000,
        };

        let nev = unsafe {
            kevent(
                kqueue,
                std::ptr::null(),
                0,
                events.as_mut_ptr() as *mut _,
                events.len() as i32,
                &timeout as *const _,
            )
        };

        if nev < 0 {
            eprintln!(
                "[!] kqueue wait failed: {}",
                std::io::Error::last_os_error()
            );
            continue;
        }

        for i in 0..nev as usize {
            process_event(server, kqueue, &events[i]);
        }

        // Periodically check for idle clients
        cleanup_idle_clients(server);
    }
}
