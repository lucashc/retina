//! This module implements a simple packet storage solution for packets that have been filtered.
//! It simply writes flows to a directory.
//! Each flow is identified with a unique `Flow` object that gets converted into a filename.
//! When a packet is received, it gets appended to this file by first writing a `u64` number indicating the number of bytes that are in the packet and then adding th ebytes of the packet.
use std::io::Write;

use std::{path::PathBuf, sync::mpsc::Receiver};

use std::fs;
use std::fs::OpenOptions;

use crate::{protocols::layer4::Flow, subscription::ZcFrame};

pub struct PacketStore {
    path: PathBuf,
    receiver: Receiver<(Flow, ZcFrame)>,
}

impl PacketStore {
    pub fn new(path: PathBuf, receiver: Receiver<(Flow, ZcFrame)>) -> PacketStore {
        PacketStore { path, receiver }
    }

    /// This function runs a loop to receive packets on the receiver channel.
    pub fn start_saving_loop(&self) {
        // Create directory
        if !self.path.exists() {
            fs::create_dir(&self.path).unwrap();
        }

        // Start receive loop and append to files
        for (flow, packet) in self.receiver.iter() {
            let path = self.path.join(flow.to_filename());
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            file.write(&(packet.data_len() as u64).to_le_bytes())
                .unwrap();
            file.write(packet.data()).unwrap();
        }
    }
}
