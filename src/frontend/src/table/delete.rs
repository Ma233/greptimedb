// Copyright 2023 Greptime Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use api::v1::DeleteRequest as GrpcDeleteRequest;
use common_query::Output;
use futures::future;
use meta_client::rpc::TableName;
use snafu::ResultExt;
use store_api::storage::RegionNumber;
use table::requests::DeleteRequest;

use crate::error::{JoinTaskSnafu, RequestDatanodeSnafu, Result};
use crate::table::insert::to_grpc_columns;
use crate::table::DistTable;

impl DistTable {
    pub(super) async fn dist_delete(&self, requests: Vec<GrpcDeleteRequest>) -> Result<Output> {
        let regions = requests.iter().map(|x| x.region_number).collect::<Vec<_>>();
        let instances = self.find_datanode_instances(&regions).await?;

        let results = future::try_join_all(instances.into_iter().zip(requests.into_iter()).map(
            |(instance, request)| {
                common_runtime::spawn_write(async move {
                    instance
                        .grpc_delete(request)
                        .await
                        .context(RequestDatanodeSnafu)
                })
            },
        ))
        .await
        .context(JoinTaskSnafu)?;

        let affected_rows = results.into_iter().sum::<Result<u32>>()?;
        Ok(Output::AffectedRows(affected_rows as _))
    }
}

pub(super) fn to_grpc_delete_request(
    table_name: &TableName,
    region_number: RegionNumber,
    request: DeleteRequest,
) -> Result<GrpcDeleteRequest> {
    let (key_columns, row_count) = to_grpc_columns(&request.key_column_values)?;
    Ok(GrpcDeleteRequest {
        table_name: table_name.table_name.clone(),
        region_number,
        key_columns,
        row_count,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use api::v1::column::{SemanticType, Values};
    use api::v1::{Column, ColumnDataType};
    use datatypes::prelude::VectorRef;
    use datatypes::vectors::Int32Vector;

    use super::*;

    #[test]
    fn test_to_grpc_delete_request() {
        let table_name = TableName {
            catalog_name: "greptime".to_string(),
            schema_name: "public".to_string(),
            table_name: "foo".to_string(),
        };
        let region_number = 1;

        let key_column_values = HashMap::from([(
            "id".to_string(),
            Arc::new(Int32Vector::from_slice(vec![1, 2, 3])) as VectorRef,
        )]);
        let request = DeleteRequest { key_column_values };

        let result = to_grpc_delete_request(&table_name, region_number, request).unwrap();

        assert_eq!(result.table_name, "foo");
        assert_eq!(result.region_number, region_number);
        assert_eq!(
            result.key_columns,
            vec![Column {
                column_name: "id".to_string(),
                semantic_type: SemanticType::Field as i32,
                values: Some(Values {
                    i32_values: vec![1, 2, 3],
                    ..Default::default()
                }),
                null_mask: vec![0],
                datatype: ColumnDataType::Int32 as i32,
            }]
        );
        assert_eq!(result.row_count, 3);
    }
}
