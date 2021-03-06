use std::io;
use std::io::Result as IOResult;
use std::io::{Read, Write};
use std::error::Error;
use std::u16;

use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};

const FRAME_LEN_U16: u8 = 126;
const FRAME_LEN_U64: u8 = 127;

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum OpCode {
    TextFrame = 1,
    BinaryFrame = 2,
    ConnectionClose = 8,
    Ping = 9,
    Pong = 0xA
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct WebSocketFrameHeader {
    fin: bool,
    rsv1: bool,
    rsv2: bool,
    rsv3: bool,
    masked: bool,
    opcode: u8,
    payload_length: u8
}

impl WebSocketFrameHeader {
    fn new_header(len: usize, opcode: u8) -> WebSocketFrameHeader {
        WebSocketFrameHeader {
            fin: true,
            rsv1: false, rsv2: false, rsv3: false,
            masked: false,
            payload_length: Self::determine_len(len),
            opcode: opcode
        }
    }

    fn determine_len(len: usize) -> u8 {
        if len < (FRAME_LEN_U16 as usize) {
            len as u8
        } else if len < (u16::MAX as usize) {
            FRAME_LEN_U16
        } else {
            FRAME_LEN_U64
        }
    }
}

#[derive(Debug)]
pub struct WebSocketFrame {
    header: WebSocketFrameHeader,
    mask: Option<[u8; 4]>,
    pub payload: Vec<u8>
}

impl<'a> From<&'a [u8]> for WebSocketFrame {
    fn from(payload: &[u8]) -> WebSocketFrame {
        WebSocketFrame {
            header: WebSocketFrameHeader::new_header(payload.len(), OpCode::BinaryFrame as u8),
            payload: Vec::from(payload),
            mask: None
        }
    }
}

impl<'a> From<&'a str> for WebSocketFrame {
    fn from(payload: &str) -> WebSocketFrame {
        WebSocketFrame {
            header: WebSocketFrameHeader::new_header(payload.len(), OpCode::TextFrame as u8),
            payload: Vec::from(payload),
            mask: None
        }
    }
}

impl WebSocketFrame {
    pub fn write<W: Write>(&self, output: &mut W) -> IOResult<()> {
        let hdr = Self::serialize_header(&self.header);
        try!(output.write_u16::<BigEndian>(hdr));

        match self.header.payload_length {
            FRAME_LEN_U16 => try!(output.write_u16::<BigEndian>(self.payload.len() as u16)),
            FRAME_LEN_U64 => try!(output.write_u64::<BigEndian>(self.payload.len() as u64)),
            _ => {}
        }

        try!(output.write(&self.payload));
        Ok(())
    }

    pub fn read<R: Read>(input: &mut R) -> IOResult<WebSocketFrame> {
        let buf = try!(input.read_u16::<BigEndian>());
        let header = Self::parse_header(buf);

        let len = try!(Self::read_length(header.payload_length, input));
        let mask_key = if header.masked {
            let mask = try!(Self::read_mask(input));
            Some(mask)
        } else {
            None
        };
        let mut payload = try!(Self::read_payload(len, input));

        if let Some(mask) = mask_key {
            Self::apply_mask(mask, &mut payload);
        }

        Ok(WebSocketFrame {
            header: header,
            payload: payload,
            mask: mask_key
        })
    }

    fn serialize_header(hdr: &WebSocketFrameHeader) -> u16 {
        let b1 = ((hdr.fin as u8) << 7)
                  | ((hdr.rsv1 as u8) << 6)
                  | ((hdr.rsv2 as u8) << 5)
                  | ((hdr.rsv3 as u8) << 4)
                  | ((hdr.opcode as u8) & 0x0F);

        let b2 = ((hdr.masked as u8) << 7)
            | ((hdr.payload_length as u8) & 0x7F);

        ((b1 as u16) << 8) | (b2 as u16)
    }

    fn parse_header(buf: u16) -> WebSocketFrameHeader {
        WebSocketFrameHeader {
            fin: (buf >> 8) & 0x80 == 0x80,
            rsv1: (buf >> 8) & 0x40 == 0x40,
            rsv2: (buf >> 8) & 0x20 == 0x20,
            rsv3: (buf >> 8) & 0x10 == 0x10,
            opcode: ((buf >> 8) as u8) & 0x0F,

            masked: buf & 0x80 == 0x80,
            payload_length: (buf as u8) & 0x7F,
        }
    }

    fn apply_mask(mask: [u8; 4], bytes: &mut Vec<u8>) {
        for (idx, c) in bytes.iter_mut().enumerate() {
            *c = *c ^ mask[idx % 4];
        }
    }

    fn read_mask<R: Read>(input: &mut R) -> IOResult<[u8; 4]> {
        let mut buf = [0; 4];
        try!(input.read(&mut buf));
        Ok(buf)
    }

    fn read_payload<R: Read>(payload_len: usize, input: &mut R) -> IOResult<Vec<u8>> {
        let mut payload: Vec<u8> = Vec::with_capacity(payload_len);
        unsafe { payload.set_len(payload_len) };
        try!(input.read(&mut payload));
        Ok(payload)
    }

    fn read_length<R: Read>(payload_len: u8, input: &mut R) -> IOResult<usize> {
        return match payload_len {
            FRAME_LEN_U64 => input.read_u64::<BigEndian>().map(|v| v as usize).map_err(|e| io::Error::from(e)),
            FRAME_LEN_U16 => input.read_u16::<BigEndian>().map(|v| v as usize).map_err(|e| io::Error::from(e)),
            _ => Ok(payload_len as usize) // payload_len < 127
        }
    }
}
