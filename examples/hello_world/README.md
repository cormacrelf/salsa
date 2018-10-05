The `hello_world` example is intended to walk through the very basics
of a salsa setup. Here is a more detailed writeup.

### Step 1: Define the database trait

The **database** is the central struct that holds all the state for
your application. It has the current values of all your inputs, the
values of any memoized queries you have executed thus far, and
dependency information between them.

In your program, however, you rarely interact with the **actual**
database struct. Instead, you interact with database **traits** that
you define. These traits define the set of queries that you need for
any given piece of code. You define them using the
`salsa::query_prototype!` macro.

Here is a simple example of a database trait from the `hello_world`
example. It defines exactly two queries: `input_string` and
`length`. You see that the `query_prototype!` macro just lists out the
names of the queries as methods (e.g., `input_string()`) and also a
path to a type that will define the query (`InputString`). It doesn't
give many other details: those are specified in the query definition
that comes later. XXX out of date

```rust
salsa::query_prototype! {
    trait HelloWorldDatabase: salsa::Database {
        fn input_string(key: ()) -> Arc<String> {
            type InputString;
            storage input;
        }

        fn length(key: ()) -> usize {
            type Length;
        }
    }
}
```

### Step 2: Define the query bodies

The actual query definitions are made using the
`salsa::query_definition` macro. For an **input query**, such as
`input_string`, these resemble a variable definition:

```rust
salsa::query_definition! {
    InputString: Map<(), Arc<String>>;
}
```

Here, the `Map` is actually a keyword -- you have to write it.  The
idea is that each query isn't defining a single value: they are always
a mapping from some **key** to some **value** -- in this case, though,
the type of the key is just the unit type `()` (so in a sense this
*is* a single value). The value type would be `Arc<String>`.

Note that both keys and values are cloned with relative frequency, so
it's a good idea to pick types that can be cheaply cloned. Also, for
the incremental system to work, keys and value types must not employ
"interior mutability" (no `Mutex` or `AtomicUsize` etc).

Next let's define the `length` query, which is a function query:

```rust
salsa::query_definition! {
    Length(db: &impl HelloWorldDatabase, _key: ()) -> usize {
        // Read the input string:
        let input_string = db.input_string().get(());

        // Return its length:
        input_string.len()
    }
}
```

Like the `InputString` query, `Length` has a **key** and a **value**
-- but this time the type of the key is specified as the type of the
second argument (`_key`), and the type of the value is specified from
the return type (`usize`).

You can also see that functions take a first argument, the `db`, which
always has the form `&impl <SomeDatabaseTrait>`. This `db` value gives
access to all the other queries that are listed in the context trait
that you specify.

In the first line of the function we see how we invoke a query:

```rust
let input_string = db.input_string().get(());
```

When you invoke `db.input_string()`, what you get back is called
a `QueryTable` -- it offers a few methods that let you interact with
the query. The main method you will use though is `get(key)` which --
given a `key` -- computes and returns the up-to-date value. In the
case of an input query like `input_string`, this just returns whatever
value has been set by the user (if no value has been set yet, it
returns the `Default::default()` value; all query inputs must
implement `Default`).

### Step 3: Define the database struct that implements the database trait

The final step is to create the **database struct** which will
implement your database trait(s). This struct combines all the parts
of your system into one whole; it can also add custom state of your
own (such as an interner or configuration). In our simple example
though we won't do any of that. The only field that you **actually**
need is a reference to the **salsa runtime**; then you must also
implement the `salsa::Database` trait to tell salsa where to find this
runtime:

```rust
#[derive(Default)]
struct DatabaseStruct {
    runtime: salsa::runtime::Runtime<DatabaseStruct>,
}

impl salsa::Database for DatabaseStruct {
    fn salsa_runtime(&self) -> &salsa::runtime::Runtime<DatabaseStruct> {
        &self.runtime
    }
}
```

Next, you must use the `database_storage!` to define the "storage
struct" for your type. This storage struct contains all the hashmaps
and other things that salsa uses to store the values for your
queries. You won't need to interact with it directly. To use the
macro, you basically list out all the traits and each of the queries
within those traits:

```rust
salsa::database_storage! {
    struct DatabaseStorage for DatabaseStruct {
    //     ^^^^^^^^^^^^^^^     --------------
    //     name of the type    the name of your context type
    //     we will make
        impl HelloWorldDatabase {
            fn input_string() for InputString;
            fn length() for Length;
        }
    }
}
```

The `database_storage` macro will also implement the
`HelloWorldDatabase` trait for your query context type.

Now that we've defined our database, we can start using it:

```rust
fn main() {
    let db = DatabaseStruct::default();

    println!("Initially, the length is {}.", db.length().get(()));

    db.input_string().set((), Arc::new(format!("Hello, world")));

    println!("Now, the length is {}.", db.length().get(()));
}
```

And if we run this code:

```bash
> cargo run --example hello_world
   Compiling salsa v0.2.0 (/Users/nmatsakis/versioned/salsa)
    Finished dev [unoptimized + debuginfo] target(s) in 0.94s
     Running `target/debug/examples/hello_world`
Initially, the length is 0.
Now, the length is 12.
```

Amazing.
