use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Table {
    pub name: String,
    pub rows: u64,
    pub size: u64,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Column {
    pub name: String,
    pub format: Format,
    pub length: u64
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Index {
    pub name: String,
    pub size: u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Integer,
    Numeric,
    Real,
    Boolean,
    Text,
    Blob
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Integer => f.write_str("integer"),
            Self::Numeric => f.write_str("numeric"),
            Self::Real    => f.write_str("real"),
            Self::Boolean => f.write_str("boolean"),
            Self::Text    => f.write_str("text"),
            Self::Blob    => f.write_str("blob")
        }
    }
}

impl FromStr for Format {
    type Err = anyhow::Error;

    fn from_str(format: &str) -> Result<Self, Self::Err> {
        let format = format.to_ascii_lowercase();

        // https://sqlite.org/datatype3.html
        let formats = [
            (Self::Integer, vec!["integer", "tinyint", "smallint", "mediumint", "bigint", "unsigned big int", "int2", "int8"]),
            (Self::Numeric, vec!["numeric", "date"]),
            (Self::Real,    vec!["real", "float", "double", "decimal"]),
            (Self::Boolean, vec!["boolean"]),
            (Self::Text,    vec!["text", "char", "varchar", "varying character", "nchar", "native character", "nvarchar", "clob", "timestamp"]),
            (Self::Blob,    vec!["blob"])
        ];

        for (name, definitions) in formats {
            for definition in definitions {
                if definition.starts_with(format.as_str()) || format.starts_with(definition) {
                    return Ok(name);
                }
            }
        }

        anyhow::bail!("Invalid column data type: {format}")
    }
}

pub fn query_structure(connection: &rusqlite::Connection) -> anyhow::Result<Vec<Table>> {
    let mut query = connection.prepare("
        SELECT
            sqlite_schema.name AS table_name,
            SUM(dbstat.pgsize) AS bytes
        FROM dbstat
        JOIN sqlite_schema
        ON dbstat.name = sqlite_schema.name
        WHERE sqlite_schema.type = 'table'
        GROUP BY sqlite_schema.name;
    ")?;

    let mut tables_raw = query.query_map([], |row| {
        let table_name = row.get::<_, String>("table_name")?;
        let bytes = row.get::<_, u64>("bytes")?;

        Ok((table_name, bytes))
    })?.collect::<Result<Vec<_>, _>>()?;

    let mut tables = Vec::with_capacity(tables_raw.len());

    for (table, size) in tables_raw.drain(..) {
        let rows = connection.prepare(&format!("SELECT COUNT(rowid) AS rows FROM `{table}`"))?
            .query_row([], |row| row.get::<_, u64>("rows"))?;

        let mut query = connection.prepare(&format!("SELECT name, type FROM pragma_table_info('{table}')"))?;

        let mut columns_raw = query.query_map([], |row| {
            let name = row.get::<_, String>("name")?;
            let format = row.get::<_, String>("type")?;

            Ok((name, format))
        })?.map(|row| {
            row.map_err(|err| anyhow::anyhow!(err))
                .and_then(|(name, format)| Ok((name, Format::from_str(&format)?)))
        }).collect::<Result<Vec<_>, _>>()?;

        let mut columns = Vec::with_capacity(columns_raw.len());

        for (column, format) in columns_raw.drain(..) {
            let mut query = connection.prepare(&format!("
                SELECT IFNULL(SUM(LENGTH(`{column}`)), 0) AS size
                FROM `{table}` WHERE `{column}` IS NOT NULL
            "))?;

            let length = query.query_row([], |row| row.get::<_, u64>("size"))?;

            columns.push(Column {
                name: column,
                format,
                length
            });
        }

        let mut query = connection.prepare(&format!("SELECT name FROM pragma_index_list('{table}')"))?;

        let mut indexes_raw = query.query_map([], |row| row.get::<_, String>("name"))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut indexes = Vec::with_capacity(indexes_raw.len());

        for index in indexes_raw.drain(..) {
            let mut query = connection.prepare(&format!("
                SELECT SUM(dbstat.pgsize) AS size FROM dbstat
                JOIN sqlite_schema
                ON dbstat.name = sqlite_schema.name
                WHERE
                    sqlite_schema.type = 'index' AND
                    sqlite_schema.name = '{index}'
                GROUP BY sqlite_schema.name;
            "))?;

            let size = query.query_row([], |row| row.get::<_, u64>("size"))?;

            indexes.push(Index {
                name: index,
                size
            });
        }

        tables.push(Table {
            name: table,
            size,
            rows,
            columns,
            indexes
        });
    }

    Ok(tables)
}
