use std::{net::TcpListener, thread, time::{Duration, Instant}};

use crate::{clock::ClockManager, net::{server::Server, tcp::build_tcp_controller}, simulation::TICK_DT};


pub fn run_network_server_loop(mut clock_manager: ClockManager, port: u16) {
    let listener = TcpListener::bind(("0.0.0.0", port)).unwrap_or_else(|err| {
        panic!("failed to bind tcp listener on port {port}: {err}");
    });
    listener
        .set_nonblocking(true)
        .expect("failed to set tcp listener nonblocking");

    let tick_duration = Duration::from_secs_f64(TICK_DT);
    let mut next_tick = Instant::now() + tick_duration;
    let mut next_controller_id = 1u64;

    loop {
        accept_pending_clients(&listener, clock_manager.server_mut().expect("server should exist"));

        let now = Instant::now();
        if now < next_tick {
            thread::sleep(next_tick - now);
            continue;
        }

        clock_manager.advance(TICK_DT);
        next_tick += tick_duration;

        let catch_up_limit = now + tick_duration;
        if next_tick < catch_up_limit {
            next_tick = catch_up_limit;
        }
    }
}

fn accept_pending_clients(listener: &TcpListener, server: &mut Server) {
    loop {
        match listener.accept() {
            Ok((stream, _addr)) => {
                let controller_id = server.allocate_controller_id();
                let controller = build_tcp_controller(stream, controller_id);
                server.add_controller(Box::new(controller), 21.0, 11.0);
            }
            Err(_) => break,
        }
    }
}
