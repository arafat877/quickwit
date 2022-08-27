use std::io;
use std::path::Path;

use super::mem;
use super::record::ReadRecordError;
use super::rolling;
use super::rolling::RecordLogReader;
use super::Record;

pub struct MultiRecordLog {
    record_log_writer: rolling::RecordLogWriter,
    in_mem_queues: mem::MemQueues,
    last_position: u64,
}

impl MultiRecordLog {
    pub async fn open(directory_path: &Path) -> Result<Self, ReadRecordError> {
        let mut record_log_reader = RecordLogReader::open(directory_path).await?;
        let mut in_mem_queues = mem::MemQueues::default();
        let mut last_position = 0u64;
        while let Some(record) = record_log_reader.read_record().await? {
            match record {
                Record::AddRecord {
                    position,
                    queue,
                    payload,
                } => {
                    in_mem_queues.add_record(queue, position, payload);
                    last_position = position;
                }
                Record::Truncate { position, queue } => {
                    in_mem_queues.truncate(queue, position);
                    last_position = position;
                }
            }
        }
        let record_log_writer = record_log_reader.into_writer();
        Ok(MultiRecordLog {
            record_log_writer,
            in_mem_queues,
            last_position,
        })
    }

    pub fn num_files(&self) -> usize {
        self.record_log_writer.num_files()
    }

    // Returns a new position.
    fn inc_position(&mut self) -> u64 {
        self.last_position += 1;
        self.last_position
    }

    pub async fn append_record(&mut self, queue: &str, payload: &[u8]) -> io::Result<()> {
        let position = self.inc_position();
        let record = Record::AddRecord {
            position,
            queue,
            payload,
        };
        self.record_log_writer.write_record(record).await?;
        self.record_log_writer.flush().await?;
        self.in_mem_queues.add_record(queue, position, payload);
        Ok(())
    }

    /// Returns the first record with position greater of equal to position.
    pub fn get_after(&self, queue_id: &str, position: u64) -> Option<(u64, &[u8])> {
        self.in_mem_queues.get_after(queue_id, position)
    }

    pub async fn truncate(&mut self, queue: &str, position: u64) -> io::Result<()> {
        self.record_log_writer
            .write_record(Record::Truncate { position, queue })
            .await?;
        if let Some(position) = self.in_mem_queues.truncate(queue, position) {
            self.record_log_writer.truncate(position).await?;
        }
        Ok(())
    }
}
