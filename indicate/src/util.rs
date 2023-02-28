use std::{collections::BTreeMap, sync::Arc};

use trustfall::{FieldValue, TransparentValue};

/// Transform a result from [`execute_query`] to one where the fields can easily
/// be serialized to JSON using [`TransparentValue`].
pub fn transparent_results(
    res: Vec<BTreeMap<Arc<str>, FieldValue>>,
) -> Vec<BTreeMap<Arc<str>, TransparentValue>> {
    res.into_iter()
        .map(|entry| entry.into_iter().map(|(k, v)| (k, v.into())).collect())
        .collect()
}
