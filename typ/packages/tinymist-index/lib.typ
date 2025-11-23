
#let index-plugin = plugin("tinymist_index.wasm")

/// Creates an index.
/// - db-data (bytes): The data of the (database) index.
#let create_index(db-data) = plugin.transition(index-plugin.create_index, db-data, bytes(""))

/// Queries the index.
/// - db (any): The database.
/// - kind (str): The kind of the query.
/// - request (any): The request for the query.
#let query(db, kind, request) = json(db.query_index(bytes(kind), bytes(json.encode(request))))
