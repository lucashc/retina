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
