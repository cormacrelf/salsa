use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

#[salsa::query_group(HelloWorldStorage)]
trait HelloWorld<Generic>: salsa::Database {
    #[salsa::input]
    fn input_yo(&self, key: ()) -> Arc<Generic>;
    fn yo(&self, key: ()) -> Arc<Generic>;
}

fn yo<T: Default + Hash + Debug + Eq + Clone + Sync + Send + Sized + 'static>(
    db: &impl HelloWorld<T>,
    _key: (),
) -> Arc<T> {
    db.input_yo(()).clone()
}

#[salsa::database(HelloWorldStorage<T>)]
#[derive(Default)]
struct DatabaseStruct<T: Debug + Hash + Default + Clone + Eq + PartialEq + Send + Sync + 'static> {
    runtime: salsa::Runtime<DatabaseStruct<T>>,
}

impl<T: Debug + Hash + Default + Eq + PartialEq + Clone + Send + Sync + 'static> salsa::Database
    for DatabaseStruct<T>
{
    fn salsa_runtime(&self) -> &salsa::Runtime<DatabaseStruct<T>> {
        &self.runtime
    }
}

fn main() {
    let mut db: DatabaseStruct<Option<String>> = DatabaseStruct::default();

    db.set_input_yo((), Arc::new(Some("we used Option<String>".into())));

    println!("{:?}.", db.yo(()));
}
