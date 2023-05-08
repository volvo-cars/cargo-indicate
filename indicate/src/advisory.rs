use std::path::Path;

use cvss::Severity;
use rustsec::{
    database::Query,
    package::Name,
    platforms::{Arch, OS},
    Advisory, Database,
};

/// Wrapper around an advisory database used to perform queries
#[derive(Debug)]
pub struct AdvisoryClient {
    db: Database,
}

impl AsRef<Database> for AdvisoryClient {
    fn as_ref(&self) -> &Database {
        &self.db
    }
}

impl From<Database> for AdvisoryClient {
    fn from(value: Database) -> Self {
        Self { db: value }
    }
}

impl From<AdvisoryClient> for Database {
    fn from(value: AdvisoryClient) -> Self {
        value.db
    }
}

impl AdvisoryClient {
    /// Creates a new client by fetching the default database from GitHub
    ///
    /// It is a good idea to create this lazily (for example using [`OnceCell`]
    /// (`once_cell::unsync::OnceCell`)) since the operation is costly when not
    /// needed.
    ///
    /// # Errors
    ///
    /// If the default advisory database cannot be fetched, an error variant
    /// will be returned.
    pub fn new() -> Result<Self, rustsec::Error> {
        let db = Database::fetch()?;
        Ok(Self { db })
    }

    /// Create a new client from a advisory database file
    ///
    /// # Errors
    ///
    /// If an advisory database cannot be opened at the provided path, an error
    /// variant will be returned.
    pub fn from_path(path: &Path) -> Result<Self, rustsec::Error> {
        let db = Database::open(path)?;
        Ok(Self { db })
    }

    /// Create a client from the default local path in `CARGO_HOME` directory
    /// (`~./cargo/advisory-db`)
    pub fn from_default_path() -> Result<Self, rustsec::Error> {
        let default = format!("{}/advisory-db", env!("CARGO_HOME"));
        Self::from_path(Path::new(default.as_str()))
    }

    /// Retrieves all advisories for a package
    ///
    /// See also the `advisoryHistory` edge for the `Package`
    /// [`Vertex`](crate::vertex::Vertex).
    #[must_use]
    pub fn all_advisories_for_package(
        &self,
        name: Name,
        include_withdrawn: bool,
        arch: Option<Arch>,
        os: Option<OS>,
        min_severity: Option<Severity>,
    ) -> Vec<&Advisory> {
        let mut query = Query::new().package_name(name);

        if let Some(arch) = arch {
            query = query.target_arch(arch);
        }

        if let Some(os) = os {
            query = query.target_os(os);
        }

        if let Some(min_severity) = min_severity {
            query = query.severity(min_severity);
        }

        let mut res = self.db.query(&query);

        // Append withdrawn
        if include_withdrawn {
            query = query.withdrawn(include_withdrawn);
            res.append(&mut self.db.query(&query));
        }

        res
    }
}
