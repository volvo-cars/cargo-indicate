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
pub(crate) struct AdvisoryClient {
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
    pub fn new() -> Result<Self, rustsec::Error> {
        let db = Database::fetch()?;
        Ok(Self { db })
    }

    /// Create a new client from a advisory database file
    pub fn from_path(path: &Path) -> Result<Self, rustsec::Error> {
        let db = Database::open(path)?;
        Ok(Self { db })
    }

    /// Retrieves all advisories for a package
    ///
    /// See also the `advisoryHistory` edge for the `Package`
    /// [`Vertex`](crate::vertex::Vertex).
    pub fn all_advisories_for_package(
        &self,
        name: Name,
        withdrawn: bool,
        arch: Option<Arch>,
        os: Option<OS>,
        severity: Option<Severity>,
    ) -> Vec<&Advisory> {
        let mut query = Query::new().package_name(name).withdrawn(withdrawn);

        if let Some(arch) = arch {
            query = query.target_arch(arch);
        }

        if let Some(os) = os {
            query = query.target_os(os);
        }

        if let Some(severity) = severity {
            query = query.severity(severity);
        }

        self.db.query(&query)
    }
}
