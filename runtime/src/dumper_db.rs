use {
    postgres::{Client, NoTls, Statement},
};

pub struct DumperDb {
    client: Client,
}