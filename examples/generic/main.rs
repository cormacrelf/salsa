use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

#[salsa::query_group(HelloWorldStorage)]
trait HelloWorld: salsa::Database {
    type Assoc: Default + Clone + Hash + Debug + Eq + Sync + Send;

    fn arbitrary(&self, key: ()) -> Arc<Self::Assoc>;
}

fn arbitrary<H: HelloWorld>(db: &H, key: ()) -> H::Assoc {
    H::Assoc::default()
}

#[salsa::database(HelloWorldStorage<Option<String>>)]
#[derive(Default)]
struct DatabaseStruct {
    runtime: salsa::Runtime<DatabaseStruct>,
}

impl salsa::Database for DatabaseStruct {
    fn salsa_runtime(&self) -> &salsa::Runtime<DatabaseStruct> {
        &self.runtime
    }
}

fn main() {
    let mut db: DatabaseStruct = DatabaseStruct::default();

    println!("{:?}.", db.arbitrary(()));
}
