use domain::id::Id;

pub(crate) fn to_row_id<T>(id: Id<T>) -> i64 {
    u64::from(id) as i64
}

pub(crate) fn from_row_id<T>(value: i64) -> Id<T> {
    (value as u64).into()
}
