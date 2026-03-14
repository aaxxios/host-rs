use std::error::Error;
use std::fmt;

/// Boxed error type accepted from constructors and factories.
pub type BoxError = Box<dyn Error + Send + Sync>;

/// Why a service could not be resolved.
///
/// Every variant carries the dependency path: type names from the service that
/// was requested down to the one the error is about.
#[derive(Debug)]
#[non_exhaustive]
pub enum ResolveError {
    /// The last service in `path` has no registration.
    NotRegistered {
        /// Dependency path.
        path: Vec<&'static str>,
    },
    /// The last service in `path` is scoped but was requested outside a scope,
    /// either from the root provider or by a singleton.
    ScopedFromRoot {
        /// Dependency path.
        path: Vec<&'static str>,
    },
    /// The services in `path` depend on each other; the first and last entries
    /// are the same service.
    Cycle {
        /// Dependency path.
        path: Vec<&'static str>,
    },
    /// The constructor of the last service in `path` returned an error.
    Construction {
        /// Dependency path.
        path: Vec<&'static str>,
        /// The constructor's error.
        source: BoxError,
    },
}

impl ResolveError {
    /// The dependency path this error refers to.
    pub fn path(&self) -> &[&'static str] {
        match self {
            Self::NotRegistered { path }
            | Self::ScopedFromRoot { path }
            | Self::Cycle { path }
            | Self::Construction { path, .. } => path,
        }
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotRegistered { .. } => "service not registered",
            Self::ScopedFromRoot { .. } => "scoped service resolved outside a scope",
            Self::Cycle { .. } => "dependency cycle",
            Self::Construction { .. } => "failed to construct service",
        };
        write!(f, "{message}: ")?;
        write_path(f, self.path())
    }
}

impl Error for ResolveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Construction { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// The service collection was rejected by `build`.
#[derive(Debug)]
pub struct BuildError {
    issues: Vec<BuildIssue>,
}

impl BuildError {
    /// Every problem found, in registration order.
    pub fn issues(&self) -> &[BuildIssue] {
        &self.issues
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, issue) in self.issues.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{issue}")?;
        }
        Ok(())
    }
}

impl Error for BuildError {}

/// A single problem found while building a service collection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuildIssue {
    /// The services in `path` depend on each other; the first and last entries
    /// are the same service.
    Cycle {
        /// Dependency path.
        path: Vec<&'static str>,
    },
}

impl fmt::Display for BuildIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cycle { path } => {
                f.write_str("dependency cycle: ")?;
                write_path(f, path)
            }
        }
    }
}

fn write_path(f: &mut fmt::Formatter<'_>, path: &[&'static str]) -> fmt::Result {
    for (i, name) in path.iter().enumerate() {
        if i > 0 {
            f.write_str(" -> ")?;
        }
        f.write_str(name)?;
    }
    Ok(())
}
