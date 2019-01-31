use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

trait Carrier: Default + Debug + Clone + Eq + Hash + Send + Sync + 'static {
    type Assoc: Default + Debug + Clone + Eq + Hash + Send + Sync + 'static;
}
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
struct OptStringCarrier;
impl Carrier for OptStringCarrier {
    type Assoc = Option<String>;
}

#[salsa::query_group(HelloWorldStorage)]
trait HelloWorld<T: Carrier>: salsa::Database {
    #[salsa::input]
    fn input_yo(&self) -> Arc<T::Assoc>;
    fn yo(&self) -> Arc<T::Assoc>;
}

fn yo<T: Carrier>(db: &impl HelloWorld<T>) -> Arc<T::Assoc> {
    db.input_yo().clone()
}

#[salsa::database(HelloWorldStorage<T>)]
#[derive(Default)]
struct DatabaseStruct<T: Carrier> {
    runtime: salsa::Runtime<DatabaseStruct<T>>,
}

impl<T: Carrier> salsa::Database for DatabaseStruct<T> {
    fn salsa_runtime(&self) -> &salsa::Runtime<DatabaseStruct<T>> {
        &self.runtime
    }
}

fn main() {
    let mut db: DatabaseStruct<OptStringCarrier> = DatabaseStruct::default();

    db.set_input_yo(Arc::new(Some("we used Option<String>".into())));

    println!("{:?}.", db.yo());
}
