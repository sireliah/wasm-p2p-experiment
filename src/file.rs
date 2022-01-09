use std::error::Error;
use std::fmt;
use std::io::{self, Read, Write};

use std::fs::{metadata, File};
use std::path::Path;

use libp2p::core::PeerId;

use crate::log;
use crate::console_log;
use crate::peer::TransferType;

#[derive(Clone, Debug)]
pub enum Payload {
    Path(String),
    Text(String),
}

impl Payload {
    pub fn new(transfer_type: TransferType, path: String) -> Result<Payload, io::Error> {
        match transfer_type {
            TransferType::File => Ok(Payload::Path(path)),
            TransferType::Text => {
                let mut file = File::open(path)?;
                let mut contents = String::new();
                let _ = file.read_to_string(&mut contents);

                Ok(Payload::Text(contents))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileToSend {
    pub peer: PeerId,
    pub name: String,
    pub payload: Payload,
    pub transfer_type: TransferType,
}

impl FileToSend {
    pub fn new(peer: &PeerId, payload: Payload) -> Result<Self, Box<dyn Error>> {
        console_log!("Got a payload! {}", payload);
        match payload {
            Payload::Path(path) => {
                let name = Self::extract_name_path(&path)?;
                Ok(FileToSend {
                    name,
                    payload: Payload::Path(path),
                    peer: peer.to_owned(),
                    transfer_type: TransferType::File,
                })
            }
            Payload::Text(text) => {
                let name = Self::extract_name_text(&text);
                Ok(FileToSend {
                    name,
                    payload: Payload::Text(text),
                    peer: peer.to_owned(),
                    transfer_type: TransferType::Text,
                })
            }
        }
    }

    // pub fn get_file(&self) -> Result<File, io::Error> {
    //     match &self.payload {
    //         Payload::Text(text) => Ok(Self::create_temp_file(text)?),
    //         Payload::Path(path) => Ok(File::open(path)?),
    //     }
    // }

    // pub async fn calculate_hash(&self) -> Result<String, io::Error> {
    //     get_hash_from_payload(&self.payload)
    // }

    // pub fn check_size(&self) -> Result<u64, io::Error> {
    //     match &self.payload {
    //         Payload::Path(path) => {
    //             let meta = metadata(path)?;
    //             Ok(meta.len())
    //         }
    //         Payload::Text(text) => Ok(text.len() as u64),
    //     }
    // }

    // /// Creates temporary file from text payload, so this kind of payload
    // /// can be treated as file by the transfer protocol.
    // pub fn create_temp_file(text: &str) -> Result<File, io::Error> {
    //     let mut tmp_file = NamedTempFile::new()?;
    //     tmp_file.write(text.as_bytes())?;
    //     let file = tmp_file.reopen()?;
    //     Ok(file)
    // }

    fn extract_name_text(text: &str) -> String {
        match text.get(0..5) {
            Some(t) => format!("{} (...)", t).replace("\n", ""),
            None => "text".to_string(),
        }
    }

    fn extract_name_path(path: &str) -> Result<String, Box<dyn Error>> {
        let path = Path::new(path).canonicalize()?;
        let name = path
            .file_name()
            .expect("There is no file name")
            .to_str()
            .expect("Expected a name")
            .to_string();
        Ok(name)
    }
}

impl fmt::Display for FileToSend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FileToSend name: {}, type: {}",
            self.name, self.transfer_type
        )
    }
}

impl fmt::Display for Payload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => write!(f, "PathPayload({})", path),
            Self::Text(text) => write!(f, "TextPayload({})", text.len()),
        }
    }
}

// pub fn get_hash_from_payload(payload: &Payload) -> Result<String, io::Error> {
//     match payload {
//         Payload::Path(path) => {
//             let file = File::open(&path)?;
//             Ok(hash_contents(file)?)
//         }
//         Payload::Text(text) => {
//             let file = FileToSend::create_temp_file(text)?;
//             Ok(hash_contents(file)?)
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use crate::p2p::transfer::file::FileToSend;

    #[test]
    fn test_extract_name_text() {
        let text = "here is the text I'm sending";
        let result = FileToSend::extract_name_text(text);

        assert_eq!(result, "here  (...)");
    }
}
