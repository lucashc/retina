//! This module implements a simple packet storage solution for packets that have been filtered.
//! It simply writes flows to a directory.
//! Each flow is identified with a unique `Flow` object that gets converted into a filename.
//! When a packet is received, it gets appended to this file by first writing a `u64` number indicating the number of bytes that are in the packet and then adding th ebytes of the packet.
use std::io::Write;

use std::num::NonZeroUsize;
use std::{path::PathBuf, sync::mpsc::Receiver};

use std::fs;
use std::fs::File;
use std::fs::OpenOptions;

use crate::{protocols::layer4::Flow, subscription::ZcFrame};

use lru::LruCache;

pub struct PacketStore {
    path: PathBuf,
    receiver: Receiver<(Flow, ZcFrame)>,
    cache: LruCache<Flow, File>
}

impl PacketStore {
    pub fn new(path: PathBuf, receiver: Receiver<(Flow, ZcFrame)>) -> PacketStore {
        PacketStore { path, receiver, cache: LruCache::new(NonZeroUsize::new(1_000).unwrap())}
    }

    /// This function runs a loop to receive packets on the receiver channel.
    pub fn start_saving_loop(&mut self) {
        // Create directory
        if !self.path.exists() {
            fs::create_dir(&self.path).unwrap();
        }

        // Start receive loop and append to files
        for (flow, packet) in self.receiver.iter() {
            let save_file = self.cache.get_or_insert_mut(flow, || {
                let path = self.path.join(flow.to_filename());
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .unwrap()
            });
            save_file.write(&(packet.data_len() as u64).to_le_bytes())
                .unwrap();
            save_file.write(packet.data()).unwrap();
            self.cache.promote(&flow);
        }
    }
}
