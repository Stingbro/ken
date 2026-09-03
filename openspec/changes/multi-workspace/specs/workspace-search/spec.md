# workspace-search

## MODIFIED Requirements

### Requirement: Search reaches every manifest member
Routed search SHALL take its targets from the manifests of the workspaces
in scope, not from the members that happen to be open in the current
session, and SHALL treat a member of an open but non-focused workspace
exactly as it treats a dormant member of the focused one. A member whose
runtime is dormant SHALL still be searched, by opening its index by
project id, and SHALL NOT be activated — no watcher, no engine, and no
eviction of a resident — as a side effect of being searched. A member
already resident SHALL have its live database handle reused rather than
opened a second time. Members whose folder is missing or whose
configuration does not parse SHALL NOT become targets. Members of closed
workspaces SHALL NOT become targets.

#### Scenario: A dormant member contributes results
- **WHEN** a query is run over a workspace where a member has never been
  focused this session
- **THEN** that member's index is searched and its hits appear in the
  merged results, and the member is still dormant afterwards

#### Scenario: A non-focused workspace's member contributes results
- **WHEN** an all-workspaces query is run and a member belongs to an open
  but non-focused workspace
- **THEN** that member is searched, its hits appear in the merged
  results, and it is still dormant afterwards

#### Scenario: A closed workspace is out of reach
- **WHEN** an all-workspaces query is run and a workspace is closed
- **THEN** none of its members are searched

#### Scenario: A resident member is not opened twice
- **WHEN** a query is run over a workspace with a resident member
- **THEN** that member is searched through its existing database handle

#### Scenario: An unresolvable member is skipped, not fatal
- **WHEN** the manifest lists a member whose folder no longer exists
- **THEN** it is not searched, the remaining members return results, and
  the search does not fail

### Requirement: Scope control over the search
Search scope SHALL be a two-level address: a workspace tier and, within
it, a member tier. The workspace tier SHALL default to the focused
workspace, SHALL accept a single named open workspace, and SHALL accept
all open workspaces. The member tier SHALL default to every member in
scope and SHALL accept a single named member. Pinning a member SHALL
search exactly that member without consulting any knowledge graph, and
SHALL produce the same result shape as an unpinned search. Requesting all
workspaces together with a pinned member SHALL be rejected as a usage
error, since a member identifies its workspace already. When only one
workspace is open, the workspace tier SHALL NOT be shown.

#### Scenario: Default scope is the focused workspace
- **WHEN** the user searches without changing the scope
- **THEN** planning runs across the focused workspace's eligible members
  and no other workspace is consulted

#### Scenario: All-workspaces scope spans open workspaces
- **WHEN** the user selects all workspaces and searches
- **THEN** eligible members of every open workspace are searched

#### Scenario: Pinned workspace excludes the others
- **WHEN** the user pins the scope to one open workspace and searches
- **THEN** only that workspace's members are searched and no other
  workspace's knowledge graph is consulted

#### Scenario: Pinned member with all-workspaces is rejected
- **WHEN** a search requests all workspaces and also pins a member
- **THEN** the request is rejected as a usage error rather than silently
  narrowed

#### Scenario: The workspace tier hides when it is meaningless
- **WHEN** exactly one workspace is open
- **THEN** the scope control shows only the member tier

### Requirement: Every result names its project
Each merged result SHALL carry the member name it came from and its
addressable identity, and the UI SHALL show which project each result
belongs to. When more than one workspace is in scope, each result SHALL
additionally carry and display its owning workspace, so that members
sharing a name across workspaces are distinguishable.

#### Scenario: Results are attributed
- **WHEN** results merge from more than one member
- **THEN** each result displays the name of the project it came from

#### Scenario: Cross-workspace results name their workspace
- **WHEN** results merge from members in two workspaces
- **THEN** each result displays its workspace as well as its project

#### Scenario: Same-named members stay distinguishable
- **WHEN** two open workspaces each contain a member with the same name
  and both return hits
- **THEN** the results identify which workspace each hit came from

## ADDED Requirements

### Requirement: Planning federates across workspaces
When more than one workspace is in scope, route planning SHALL run once
per in-scope workspace against that workspace's own knowledge graph, and
the resulting target sets SHALL be combined before searching. No
knowledge graph spanning workspaces SHALL be created or required. A
workspace whose graph is unavailable SHALL fall back to its
non-graph planning behavior rather than failing the search.

#### Scenario: Each workspace is planned against its own graph
- **WHEN** an all-workspaces search runs over two workspaces
- **THEN** each workspace's targets are planned against its own graph and
  the targets are combined

#### Scenario: One missing graph does not fail the search
- **WHEN** one in-scope workspace has no usable knowledge graph
- **THEN** that workspace still contributes targets by its non-graph
  planning and the search completes

### Requirement: Workspace-level status is reported
The per-member status list SHALL identify the workspace each member
belongs to, and a workspace that could not be consulted at all SHALL be
reported as such, distinctly from each of its members being unavailable.

#### Scenario: An unavailable workspace is reported once
- **WHEN** an in-scope workspace cannot be consulted
- **THEN** it is reported as unavailable at the workspace level rather
  than as a list of unavailable members
