use crate::{clock::ClockManager, net::udp};

pub fn run_network_server_loop(clock_manager: ClockManager, port: u16) {
    udp::run_udp_server_loop(clock_manager, port)
}
