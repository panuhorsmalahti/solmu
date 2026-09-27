use std::{
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::net::UnixStream,
    },
};

pub fn write<T: serde::Serialize>(stream: &mut UnixStream, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > 4096 {
        return Err(io::Error::other("Network request exceeds 4096 bytes"));
    }
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)
}

pub fn read<T: serde::de::DeserializeOwned>(stream: &mut UnixStream) -> io::Result<T> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > 4096 {
        return Err(io::Error::other("Network request exceeds 4096 bytes"));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

// Descriptor passing is confined to the trusted network worker. The agent
// never inherits these channels or connected host descriptors.
pub fn send(stream: &UnixStream, bytes: &[u8], descriptor: Option<RawFd>) -> io::Result<()> {
    let mut control = [0usize; 4];
    let mut iov = libc::iovec {
        iov_base: bytes.as_ptr().cast_mut().cast(),
        iov_len: bytes.len(),
    };
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    if let Some(descriptor) = descriptor {
        message.msg_control = control.as_mut_ptr().cast();
        message.msg_controllen =
            unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
        // SAFETY: aligned, sufficiently large ancillary buffer; one valid fd.
        unsafe {
            let header = libc::CMSG_FIRSTHDR(&message);
            (*header).cmsg_level = libc::SOL_SOCKET;
            (*header).cmsg_type = libc::SCM_RIGHTS;
            (*header).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as usize;
            libc::CMSG_DATA(header).cast::<RawFd>().write(descriptor);
        }
    }
    loop {
        // SAFETY: live stream and valid data/control buffers for this call.
        let count = unsafe { libc::sendmsg(stream.as_raw_fd(), &message, libc::MSG_NOSIGNAL) };
        if count == bytes.len() as isize {
            return Ok(());
        }
        if count >= 0 {
            return Err(io::Error::other("Incomplete network descriptor message"));
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

pub fn receive(stream: &mut UnixStream, bytes: &mut [u8]) -> io::Result<Option<OwnedFd>> {
    let mut control = [0usize; 4];
    let mut iov = libc::iovec {
        iov_base: bytes.as_mut_ptr().cast(),
        iov_len: bytes.len(),
    };
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = std::mem::size_of_val(&control);
    let count = loop {
        // SAFETY: buffers are valid and aligned; kernel marks received fds CLOEXEC.
        let count =
            unsafe { libc::recvmsg(stream.as_raw_fd(), &mut message, libc::MSG_CMSG_CLOEXEC) };
        if count >= 0 {
            break count as usize;
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    };
    let mut descriptors = Vec::new();
    // SAFETY: iterate only kernel-returned ancillary messages in the buffer.
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let length = (*header)
                    .cmsg_len
                    .saturating_sub(libc::CMSG_LEN(0) as usize);
                for index in 0..length / std::mem::size_of::<RawFd>() {
                    let fd = OwnedFd::from_raw_fd(
                        libc::CMSG_DATA(header).cast::<RawFd>().add(index).read(),
                    );
                    descriptors.push(fd);
                }
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    if count == 0 || message.msg_flags & (libc::MSG_CTRUNC | libc::MSG_TRUNC) != 0 {
        return Err(io::Error::other(
            "Network broker disconnected or sent a truncated message",
        ));
    }
    if descriptors.len() > 1 {
        return Err(io::Error::other("Unexpected extra network descriptor"));
    }
    stream.read_exact(&mut bytes[count..])?;
    Ok(descriptors.pop())
}
