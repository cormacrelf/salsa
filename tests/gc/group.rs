use crate::log::HasLog;

#[salsa::query_group]
pub(crate) trait GcDatabase: salsa::Database + HasLog {
    #[salsa::input]
    fn min(&self) -> usize;

    #[salsa::input]
    fn max(&self) -> usize;

    #[salsa::input]
    fn use_triangular(&self, key: usize) -> bool;

    fn fibonacci(&self, key: usize) -> usize;

    fn triangular(&self, key: usize) -> usize;

    fn compute(&self, key: usize) -> usize;

    fn compute_all(&self) -> Vec<usize>;
}

fn fibonacci(db: &impl GcDatabase, key: usize) -> usize {
    db.log().add(format!("fibonacci({:?})", key));
    if key == 0 {
        0
    } else if key == 1 {
        1
    } else {
        db.fibonacci(key - 1) + db.fibonacci(key - 2)
    }
}

fn triangular(db: &impl GcDatabase, key: usize) -> usize {
    db.log().add(format!("triangular({:?})", key));
    if key == 0 {
        0
    } else {
        db.triangular(key - 1) + key
    }
}

fn compute(db: &impl GcDatabase, key: usize) -> usize {
    db.log().add(format!("compute({:?})", key));
    if db.use_triangular(key) {
        db.triangular(key)
    } else {
        db.fibonacci(key)
    }
}

fn compute_all(db: &impl GcDatabase) -> Vec<usize> {
    db.log().add("compute_all()");
    (db.min()..db.max()).map(|v| db.compute(v)).collect()
}
