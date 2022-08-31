#![allow(dead_code)]
use dhcp;
use nix::{self, sys::socket};
use protocol;
use rand::prelude::*;
use std::env;
use std::net::{Ipv4Addr, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::{thread, time::Duration};
//use log::{debug, error, log_enabled, info, Level, LevelFilter};
use log::{debug, LevelFilter};
use env_logger::Builder;



////////////
// MACROS //
////////////

macro_rules! enter_func {
    ( $func: literal ) => {
        const __FUNCTION__: &str = $func;
        debug!("Entered {__FUNCTION__}");
    };
}

////////////////
// CONSTANSTS //
////////////////

const BROADCAST: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 255);
// First bit is 1, the rest are reserved, thus 0
const BCAST_FLAG: u16 = 1 << 15;
const ETHERNET: u8 = 1;
const ETHERNET_MAC_LEN: u8 = 6;
const CLIENT_HOPS: u8 = 0;
const DHCP_CLIENT_PORT: u16 = 68;
const DHCP_SERVER_PORT: u16 = 67;

#[derive(PartialEq)]
enum State {
    Init,
    InitReboot,
    Rebooting,
    Selecting,
    Requesting,
    Bound,
    Renewing,
    Rebinding,
}

fn client_init() -> ClientObject {
    enter_func!("client_init");
    // TODO: record time of process start and put in object
    let interface_name: String;
    if env::args().len() > 1 {
        interface_name = std::env::args().nth(1).unwrap();
    } else {
        interface_name = "eth0".to_string();
    }

    let addrs = nix::ifaddrs::getifaddrs().unwrap();
    let iface = addrs
        .filter(|iface| iface.interface_name == interface_name)
        .next()
        .unwrap();

    let source = std::net::SocketAddr::new(
        std::net::IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        DHCP_CLIENT_PORT,
    );
    let socket = UdpSocket::bind(source).unwrap();
    let tmp_iface = interface_name.clone();
    socket::setsockopt(
        socket.as_raw_fd(),
        socket::sockopt::BindToDevice,
        &tmp_iface.into(),
    )
    .unwrap();

    let xid = random();

    ClientObject {
        state: State::Init,
        iface,
        socket,
        xid,
        caddress: Ipv4Addr::new(0, 0, 0, 0),
        saddress: Ipv4Addr::new(0, 0, 0, 0),
        cfg: RunningCfg {
            iface_name: interface_name,
            verbose: true,
        },
    }
}

fn send_dhcp_discover(client: &mut ClientObject) {
    enter_func!("send_dhcp_discover");

    let chaddr: [u8; 6] = client
        .iface
        .address
        .unwrap()
        .as_link_addr()
        .unwrap()
        .addr()
        .unwrap();

    // TODO: Should consider further abstraction of this
    let dhcp_discover = dhcp::Dhcp {
        op: dhcp::BootOps::BOOTPREQUEST,
        htype: ETHERNET,
        hlen: ETHERNET_MAC_LEN,
        hops: CLIENT_HOPS,
        xid: client.xid,
        secs: 0,
        flags: BCAST_FLAG,
        ciaddr: [0; 4],
        yiaddr: [0; 4],
        siaddr: [0; 4],
        giaddr: [0; 4],
        chaddr,
        sname: [0; 64],
        file: [0; 128],
        options: "".into(),
    };

    send_packet(dhcp_discover, client);
}

fn send_packet(packet: dhcp::Dhcp, client: &mut ClientObject) {
    let socket = &client.socket;
    let settings = protocol::Settings::default();
    let mut pipeline = protocol::wire::dgram::Pipeline::new(
        protocol::wire::middleware::pipeline::default(),
        settings,
    );

    let mut buffer = std::io::Cursor::new(Vec::new());
    pipeline.send_to(&mut buffer, &packet).unwrap();
    let destination: Ipv4Addr;
    if client.state == State::Init {
        destination = BROADCAST;
    } else {
        destination = client.saddress;
    }
    let destination =
        std::net::SocketAddr::new(std::net::IpAddr::V4(destination), DHCP_SERVER_PORT);
    socket.set_broadcast(true).unwrap();
    socket.send_to(&buffer.into_inner(), destination).unwrap();
}

fn dhcp_select(client: &mut ClientObject) {
    enter_func!("dhcp_select");
    // Wait for DHCPOFFERs and select one.
    client.state = State::Requesting;
}

fn dhcp_request(client: &mut ClientObject) {
    enter_func!("dhcp_request");
    // Check DHCPOFFER address with Arp. If not free send DHCPDECLINE and return to State::Init.
    // Send DHCPREQUEST.
    // Receive DHCPACK
    // Or Receive DHCPNACK and return to State::Init.
    // Maybe discard a DHCPOFFER and send a new DHCPREQUEST if needed.
    // Send Arp Reply
    client.state = State::Bound;
}

fn dhcp_bind(client: &mut ClientObject) {
    enter_func!("dhcp_bind");
    // Might get additional DHCPOFFERs/DHCPACK/DHCPNACK?
    // Wait for timers and go to Renew->Rebind.
    thread::sleep(Duration::from_secs(5));
    client.state = State::Renewing;
}

fn dhcp_renew(client: &mut ClientObject) {
    enter_func!("dhcp_renew");
    // T1 expired - try to renew. DHCPREQUEST->DHCPACK => State::Bound.
    // T2 expired (Unicast DHCPREQUEST didn't get answered). Go to state::Rebinding.
    // On DHCPNACK go to State::Init.
    client.state = State::Rebinding;
}

fn dhcp_rebind(client: &mut ClientObject) {
    enter_func!("dhcp_rebind");
    // T2 expired - Broadcast DHCPREQUEST, wait for DHCPACK.
    // on DHCPNACK or timeout, go to Sate::Init.
    client.state = State::Init;
}

// https://www.rfc-editor.org/rfc/rfc2131.html#section-4.4
//
//  --------                               -------
// |        | +-------------------------->|       |<-------------------+
// | INIT-  | |     +-------------------->| INIT  |                    |
// | REBOOT |DHCPNAK/         +---------->|       |<---+               |
// |        |Restart|         |            -------     |               |
//  --------  |  DHCPNAK/     |               |                        |
//     |      Discard offer   |      -/Send DHCPDISCOVER               |
// -/Send DHCPREQUEST         |               |                        |
//     |      |     |      DHCPACK            v        |               |
//  -----------     |   (not accept.)/   -----------   |               |
// |           |    |  Send DHCPDECLINE |           |                  |
// | REBOOTING |    |         |         | SELECTING |<----+            |
// |           |    |        /          |           |     |DHCPOFFER/  |
//  -----------     |       /            -----------   |  |Collect     |
//     |            |      /                  |   |       |  replies   |
// DHCPACK/         |     /  +----------------+   +-------+            |
// Record lease, set|    |   v   Select offer/                         |
// timers T1, T2   ------------  send DHCPREQUEST      |               |
//     |   +----->|            |             DHCPNAK, Lease expired/   |
//     |   |      | REQUESTING |                  Halt network         |
//     DHCPOFFER/ |            |                       |               |
//     Discard     ------------                        |               |
//     |   |        |        |                   -----------           |
//     |   +--------+     DHCPACK/              |           |          |
//     |              Record lease, set    -----| REBINDING |          |
//     |                timers T1, T2     /     |           |          |
//     |                     |        DHCPACK/   -----------           |
//     |                     v     Record lease, set   ^               |
//     +----------------> -------      /timers T1,T2   |               |
//                +----->|       |<---+                |               |
//                |      | BOUND |<---+                |               |
//   DHCPOFFER, DHCPACK, |       |    |            T2 expires/   DHCPNAK/
//    DHCPNAK/Discard     -------     |             Broadcast  Halt network
//                |       | |         |            DHCPREQUEST         |
//                +-------+ |        DHCPACK/          |               |
//                     T1 expires/   Record lease, set |               |
//                  Send DHCPREQUEST timers T1, T2     |               |
//                  to leasing server |                |               |
//                          |   ----------             |               |
//                          |  |          |------------+               |
//                          +->| RENEWING |                            |
//                             |          |----------------------------+
//                             -----------
fn start_state_machine(client: &mut ClientObject) {
    enter_func!("start_state_machine");
    loop {
        match client.state {
            State::Init => {
                let desync = rand::thread_rng().gen_range(1000..10000);
                thread::sleep(Duration::from_millis(desync));
                send_dhcp_discover(client);
                client.state = State::Selecting
            }
            State::InitReboot => {
                send_dhcp_discover(client);
                client.state = State::Rebooting
            }
            State::Rebooting => {
                //DHCPACK => State::Bound
                //DHCPNACK => State::Init
                client.state = State::Bound
            }
            State::Selecting => dhcp_select(client),
            State::Requesting => dhcp_request(client),
            State::Bound => dhcp_bind(client),
            State::Renewing => dhcp_renew(client),
            State::Rebinding => dhcp_rebind(client),
        }
    }
}

struct RunningCfg {
    iface_name: String,
    verbose: bool,
}

struct ClientObject {
    state: State,
    iface: nix::ifaddrs::InterfaceAddress,
    cfg: RunningCfg,
    socket: UdpSocket,
    xid: u32,
    // The offered/current address
    caddress: Ipv4Addr,
    saddress: Ipv4Addr,
}

fn main() {
    let mut builder = Builder::from_default_env();
    builder.filter_level(LevelFilter::Debug);
    builder.init();
    let mut client = client_init();

    start_state_machine(&mut client);
}
