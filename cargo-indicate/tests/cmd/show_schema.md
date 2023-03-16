## Show schema
```console
$ cargo-indicate --show-schema
? success
# This is not truly a GraphQL file; Instead it is a GraphQL representation of the
# types provided by indicator.

# _This is the single source of truth for `indicator`. Any deviation from it is to
# be considered a bug._

# This is the currently supported Trustfall directives. They are handled by the
# Trustfall engine.
schema {
    query: RootQuery
}
directive @filter(
    """Name of the filter operation to perform."""
    op: String!
    """List of string operands for the operator."""
    value: [String!]
) on FIELD | INLINE_FRAGMENT
directive @tag(
    """Name to apply to the given property field."""
    name: String
) on FIELD
directive @output(
    """What to designate the output field generated from this property field."""
    name: String
) on FIELD
directive @optional on FIELD
directive @recurse(
    """
    Recurse up to this many times on this edge. A depth of 1 produces the current
    vertex and its immediate neighbors along the given edge.
    """
    depth: Int!
) on FIELD
directive @fold on FIELD
directive @transform(
    """
    Name of the transformation operation to perform.
    """
    op: String!
) on FIELD

"""
This is the actual types that can be used to create queries.

Note that each GraphQL type corresponds to one `Token` variant (see `token.rs`)
"""

type RootQuery {
    RootPackage: Package!
}

# See `cargo_metadata::Package`
type Package {
    id: ID!,
    name: String!,
    version: String!,
    license: String
    repository: Webpage
    dependencies: [Package!]!
    
    # For arch and OS, see `platforms::target`
    # For severity, see `rustsec::advisory::Severity`
    advisoryHistory(includeWithdrawn: Boolean!, arch: String, os: String, minSeverity: String): [Advisory!]!
    geiger: GeigerUnsafety!
}

# `used` refers to code used by the `RootPackage`
type GeigerUnsafety {
    used: GeigerCategories!
    unused: GeigerCategories!

    # used + unused
    total: GeigerCategories!
    forbidsUnsafe: Boolean!
}

type GeigerCategories {
    functions: GeigerCount!
    exprs: GeigerCount!
    item_impls: GeigerCount!
    item_traits: GeigerCount!
    methods: GeigerCount!

    # (functions.safe + exprs.safe + ...) and (functions.unsafe + ...)
    total: GeigerCount!
}

type GeigerCount {
    safe: Int!
    unsafe: Int!
    
    # safe + unsafe
    total: Int!
    percentageUnsafe: Float!
}

interface Webpage {
    url: String!
}

interface Repository implements Webpage {
    url: String!
}

type GitHubRepository implements Repository & Webpage {
    # From Repository and Webpage
    url: String!

    owner: GitHubUser
    name: String!
    
    starsCount: Int!
    forksCount: Int!
    openIssuesCount: Int!
    
    # If the issues page is available for this repository
    hasIssues: Boolean!
    archived: Boolean!
    
    # If this is a fork
    fork: Boolean!
}

type GitHubUser {
    username: String!
    email: String!
    unixCreatedAt: Int
    followersCount: Int!
}

# Partly flattened `rustsec::advisory::Advisory`
type Advisory {
    # These fields are flattened out of `rustsec::advisory::Metadata`

    id: String!
    title: String!
    description: String!
    unixDateReported: Int!
    severity: String
    
    # These are provided by `rustsec::advisory::Affected`
    # They may be empty, so a `None` means that we do not know
    affectedArch: [String!]
    affectedOs: [String!]
    affectedFunctions: [AffectedFunctionVersions!]
    
    # These are provided by `rustsec::advisory::Versions`
    patchedVersions: [String!]!
    unaffectedVersions: [String!]!
    
    # If it was reported in error, this will indicate when it was withdrawn
    unixDateWithdrawn: Int
    #cvss: CvssBase # TODO: Add when Trustfall supports enums
}

# `Map<FunctionPath, Vec<VersionReq>>` from `rustsec::advisory::Affected`
type AffectedFunctionVersions {
    functionPath: String!
    versions: [String!]!
}


# `rustsec::advisory::Severity`
# enum Severity {
#     NONE,
#     LOW,
#     MEDIUM,
#     HIGH,
#     CRITICAL,
# }

# `cvss::v3::base::Base`
# type CvssBase {
#     minorVersion: Int!
#     attackVector: attackVector
#     attackComplexity: AttackComplexity
#     privilegesRequired: PrivilegesRequired
#     userInteraction: UserInteraction
#     scope: Scope
#     confidentiality: Confidentiality
#     integrity: Integrity
#     availability: Availability
# }

# # `cvss::v3::base::AttackVector`
# enum AttackVector {
#     PHYSICAL
#     LOCAL
#     ADJACENT
#     NETWORK
# }

# # `cvss::v3::base::AttackComplexity`
# enum AttackComplexity {
#     HIGH
#     LOW
# }

# # `cvss::v3::base::PrivilegesRequired`
# enum PrivilegesRequired {
#     HIGH
#     LOW
#     NONE
# }

# # `cvss::v3::base::UserInteraction`
# enum UserInteraction {
#     REQUIRED
#     NONE
# }

# # `cvss::v3::base::Scope`
# enum Scope {
#     UNCHANGED
#     CHANGED
# }

# # `cvss::v3::base::Confidentiality`
# enum Confidentiality {
#     NONE
#     LOW
#     HIGH
# }

# # `cvss::v3::base::Integrity`
# enum Integrity {
#     NONE
#     LOW
#     HIGH
# }

# # `cvss::v3::base::Availability`
# enum Availability {
#     NONE
#     LOW
#     HIGH
# }

```

## `--show-schema` is exclusive

```console
$ cargo-indicate --show-schema -q ''
? failed
error: the argument '--show-schema' cannot be used with one or more of the other specified arguments

Usage: cargo-indicate [OPTIONS] <--query-path <FILE>...|--query-dir <DIR>|--query <QUERY>...|--show-schema> [-- <PACKAGE>]

For more information, try '--help'.

```