#[macro_use] extern crate protocol_derive;
extern crate protocol;
//use std::net::Ipv4Addr;
//use eui48::MacAddress;

#[derive(Protocol, Clone, Debug, PartialEq)]
#[protocol(discriminant = "integer")]
#[repr(u8)]
pub enum BootOps {
    BOOTPREQUEST = 1,
    BOOTPREPLY = 2,
}

#[allow(dead_code)]
#[derive(Protocol, Clone, Debug, PartialEq)]
#[protocol(discriminant = "integer")]
#[repr(u8)]
// The order in the enum is crucial, as it is according to RFC:
// https://www.rfc-editor.org/rfc/rfc2132#section-9.6
enum DhcpMessage {
    Discover = 1,
    Offer,
    Request,
    Decline,
    Ack,
    Nack,
    Release,
    Inform,
}

pub const MAGIC_COOKIE: u32 = 991308399;
pub const DHCP_OPTION_PADD: u8 = 0;
pub const DHCP_OPTIONS_END: u8 = 255;

#[derive(Protocol, Clone, Debug, PartialEq)]
pub struct DhcpOptionTlv {
    op_type: u8,
    length: u8,
    #[protocol(length_prefix(bytes(length)))]
    value: Vec<u8>,
}


///////////////////////
//
// http://www.tcpipguide.com/free/t_DHCPMessageFormat.htm
//
//    0                   1                   2                   3
//   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//   |     op (1)    |   htype (1)   |   hlen (1)    |   hops (1)    |
//   +---------------+---------------+---------------+---------------+
//   |                            xid (4)                            |
//   +-------------------------------+-------------------------------+
//   |           secs (2)            |           flags (2)           |
//   +-------------------------------+-------------------------------+
//   |                          ciaddr  (4)                          |
//   +---------------------------------------------------------------+
//   |                          yiaddr  (4)                          |
//   +---------------------------------------------------------------+
//   |                          siaddr  (4)                          |
//   +---------------------------------------------------------------+
//   |                          giaddr  (4)                          | 28
//   +---------------------------------------------------------------+
//   |                                                               |
//   |                          chaddr  (16)                         | 44
//   |                                                               |
//   |                                                               |
//   +---------------------------------------------------------------+
//   |                                                               | 108
//   |                          sname   (64)                         |
//   +---------------------------------------------------------------+
//   |                                                               | 236
//   |                          file    (128)                        |
//   +---------------------------------------------------------------+
//   |                                                               |
//   |                          options (variable)                   |
//   +---------------------------------------------------------------+

#[derive(Protocol, Clone, Debug, PartialEq)]
pub struct Dhcp {
    pub op: BootOps,
    // https://www.iana.org/assignments/arp-parameters/arp-parameters.xhtml#arp-parameters-2
    // Probably will hardcode "1" for now
    pub htype: u8,
    // 6 for a MAC address
    pub hlen: u8,
    // 0 for a standard client. We're not a relay
    pub hops: u8,
    pub xid: u32,
    pub secs: u16,
    // Flags field
    pub flags: u16,
    pub ciaddr: [u8; 4],
    pub yiaddr: [u8; 4],
    pub siaddr: [u8; 4],
    pub giaddr: [u8; 4],
    pub chaddr: [u8; 6],
    pub sname: [u8; 64],
    pub file: [u8; 128],
    //options: Vec<DhcpOptionTlv>,
    pub options: String,
}
