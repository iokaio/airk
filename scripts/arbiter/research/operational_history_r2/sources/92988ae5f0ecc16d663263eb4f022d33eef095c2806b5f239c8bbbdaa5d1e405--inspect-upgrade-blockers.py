# SPDX-License-Identifier: Apache-2.0
import sqlite3
import json

connection = sqlite3.connect('file:C:/Users/tdotn/AppData/Local/AIRK/state/service-1.4/control/service-1.4.sqlite3?mode=ro', uri=True)
connection.execute('PRAGMA query_only=ON')
queries = [
    "SELECT name,sql FROM sqlite_master WHERE name IN ('service_jobs','service_lifecycle','service_plans')",
    "SELECT id,kind,state FROM service_jobs WHERE state IN ('pending','running')",
    "SELECT id,phase FROM service_plans WHERE phase NOT IN ('complete','failed','canceled')",
    "SELECT id,phase FROM service_lifecycle WHERE phase NOT IN ('complete','abandoned')",
    "SELECT id,kind,stage,revision FROM index_operation_receipts",
]
for query in queries:
    print(query)
    print(json.dumps(list(connection.execute(query))))
