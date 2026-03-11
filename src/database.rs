use std::{fs::File, io::ErrorKind, path::Path};

use anyhow::{Error, Result};
use redb::{Database as ReDB, ReadableDatabase, TableDefinition};

pub struct Database {
    database: ReDB,
}

const CORE_TABLE: TableDefinition<String, Vec<u8>> = TableDefinition::new("core");
const JOB_SCHEDULER_TABLE: TableDefinition<String, Vec<u8>> = TableDefinition::new("job_scheduler");
const DISCORD_TABLE: TableDefinition<String, Vec<u8>> = TableDefinition::new("discord");

pub enum Tables {
    Core,
    JobScheduler,
    Discord,
}

impl Database {
    pub fn new(database_file_path: &Path) -> Result<Self> {
        if let Err(err) = File::create_new(database_file_path)
            && err.kind() != ErrorKind::AlreadyExists
        {
            return Err(Error::new(err));
        }

        let database = ReDB::create(database_file_path)?;

        Ok(Self { database })
    }

    pub fn get(&self, table: Tables, key: String) -> Result<Option<Vec<u8>>> {
        let read_txn = self.database.begin_read()?;

        let table = read_txn.open_table(Database::get_table(table))?;

        Ok(table.get(key)?.map(|ag| ag.value()))
    }

    pub fn insert(&self, table: Tables, key: String, value: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let write_txn = self.database.begin_write()?;

        let pvalue = {
            let mut table = write_txn.open_table(Database::get_table(table))?;

            table.insert(key, value)?.map(|ag| ag.value())
        };

        write_txn.commit()?;

        Ok(pvalue)
    }

    pub fn remove(&self, table: Tables, key: String) -> Result<Option<Vec<u8>>> {
        let write_txn = self.database.begin_write()?;

        let pvalue = {
            let mut table = write_txn.open_table(Database::get_table(table))?;

            table.remove(key)?.map(|ag| ag.value())
        };

        write_txn.commit()?;

        Ok(pvalue)
    }

    fn get_table(table: Tables) -> TableDefinition<'static, String, Vec<u8>> {
        match table {
            Tables::Core => CORE_TABLE,
            Tables::JobScheduler => JOB_SCHEDULER_TABLE,
            Tables::Discord => DISCORD_TABLE,
        }
    }
}
