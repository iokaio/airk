// SPDX-License-Identifier: Apache-2.0
use redb::TableDefinition;

pub const META: TableDefinition<u8, &[u8]> = TableDefinition::new("meta");
pub const MAP_BUCKET: TableDefinition<u16, &[u8]> = TableDefinition::new("map_bucket");
pub const MAP_NODE: TableDefinition<u32, &[u8]> = TableDefinition::new("map_node");
pub const GRAPH_RECORD: TableDefinition<u32, &[u8]> = TableDefinition::new("graph_record");
pub const GRAPH_NODE: TableDefinition<u64, &[u8]> = TableDefinition::new("graph_node");
pub const LEX_TERM: TableDefinition<&[u8], &[u8]> = TableDefinition::new("lex_term");
pub const LEX_DIRECTORY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("lex_directory");
pub const LEX_BLOCK: TableDefinition<&[u8], &[u8]> = TableDefinition::new("lex_block");
pub const LEX_DOCUMENT: TableDefinition<u64, &[u8]> = TableDefinition::new("lex_document");
pub const LEX_PATH: TableDefinition<&[u8], &[u8]> = TableDefinition::new("lex_path");
pub const LEX_FORWARD: TableDefinition<&[u8], &[u8]> = TableDefinition::new("lex_forward");
pub const PENDING: TableDefinition<&[u8], &[u8]> = TableDefinition::new("pending");
