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

use std::sync::Arc;

use async_trait::async_trait;
use common_error::prelude::*;
use common_query::Output;
use datatypes::schema::Schema;
use query::parser::PromQuery;
use session::context::QueryContextRef;
use sql::statements::statement::Statement;

use crate::error::{self, Result};

pub type SqlQueryHandlerRef<E> = Arc<dyn SqlQueryHandler<Error = E> + Send + Sync>;
pub type ServerSqlQueryHandlerRef = SqlQueryHandlerRef<error::Error>;

#[async_trait]
pub trait SqlQueryHandler {
    type Error: ErrorExt;

    async fn do_query(
        &self,
        query: &str,
        query_ctx: QueryContextRef,
    ) -> Vec<std::result::Result<Output, Self::Error>>;

    async fn do_promql_query(
        &self,
        query: &PromQuery,
        query_ctx: QueryContextRef,
    ) -> Vec<std::result::Result<Output, Self::Error>>;

    // TODO(LFC): revisit this for mysql prepared statement
    async fn do_describe(
        &self,
        stmt: Statement,
        query_ctx: QueryContextRef,
    ) -> std::result::Result<Option<Schema>, Self::Error>;

    async fn is_valid_schema(
        &self,
        catalog: &str,
        schema: &str,
    ) -> std::result::Result<bool, Self::Error>;
}

pub struct ServerSqlQueryHandlerAdaptor<E>(SqlQueryHandlerRef<E>);

impl<E> ServerSqlQueryHandlerAdaptor<E> {
    pub fn arc(handler: SqlQueryHandlerRef<E>) -> Arc<Self> {
        Arc::new(Self(handler))
    }
}

#[async_trait]
impl<E> SqlQueryHandler for ServerSqlQueryHandlerAdaptor<E>
where
    E: ErrorExt + Send + Sync + 'static,
{
    type Error = error::Error;

    async fn do_query(&self, query: &str, query_ctx: QueryContextRef) -> Vec<Result<Output>> {
        self.0
            .do_query(query, query_ctx)
            .await
            .into_iter()
            .map(|x| {
                x.map_err(BoxedError::new)
                    .context(error::ExecuteQuerySnafu { query })
            })
            .collect()
    }

    async fn do_promql_query(
        &self,
        query: &PromQuery,
        query_ctx: QueryContextRef,
    ) -> Vec<Result<Output>> {
        self.0
            .do_promql_query(query, query_ctx)
            .await
            .into_iter()
            .map(|x| {
                x.map_err(BoxedError::new).with_context(|_| {
                    let query_literal = format!("{query:?}");
                    error::ExecuteQuerySnafu {
                        query: query_literal,
                    }
                })
            })
            .collect()
    }

    async fn do_describe(
        &self,
        stmt: Statement,
        query_ctx: QueryContextRef,
    ) -> Result<Option<Schema>> {
        self.0
            .do_describe(stmt, query_ctx)
            .await
            .map_err(BoxedError::new)
            .context(error::DescribeStatementSnafu)
    }

    async fn is_valid_schema(&self, catalog: &str, schema: &str) -> Result<bool> {
        self.0
            .is_valid_schema(catalog, schema)
            .await
            .map_err(BoxedError::new)
            .context(error::CheckDatabaseValiditySnafu)
    }
}
