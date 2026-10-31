use crate::error::V2Error;
use crate::repositories::storage::{StorageIndexRow, StorageIndexSort, storage_sort_column};

const ORDER_MESSAGE: &str = "This index cannot sort results by the field named in the order.";

pub(super) fn resolve_order(
    def: &StorageIndexRow,
    order: &[String],
) -> Result<Vec<(StorageIndexSort, bool)>, V2Error> {
    let sortable = StorageIndexRow::strings(&def.sortable_fields);
    let mut sorts = Vec::with_capacity(order.len());
    for raw in order {
        let (name, descending) = match raw.strip_prefix('-') {
            Some(rest) => (rest, true),
            None => (raw.as_str(), false),
        };
        let field = name.strip_prefix("value.").unwrap_or(name);
        sorts.push(match storage_sort_column(field) {
            Some(column) => (StorageIndexSort::Column(column), descending),
            None if sortable.iter().any(|declared| declared == field) => {
                (StorageIndexSort::ValueField(field.to_owned()), descending)
            }
            None => return Err(V2Error::invalid_argument(ORDER_MESSAGE)),
        });
    }
    Ok(sorts)
}
