use {
    postgres::{Client, NoTls, Statement},
};

#[derive(Debug, Default)]
pub struct DumperDb {
}