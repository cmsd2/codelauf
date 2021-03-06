Codelauf is a source code search system

[![Build Status](https://travis-ci.org/cmsd2/codelauf.svg)](https://travis-ci.org/cmsd2/codelauf)

[Documentation](https://cmsd2.github.io/rust-docs/codelauf/codelauf/)

It is a work-in-progress.
This design document describes how it will be architected.

# Codelauf

Codelauf mirrors git repositories and uses elasticsearch to index files and commits on tracked branches.

Code is passed through some language specific syntax analysers before being loaded into the index.

You can search the indexes given a commit id or a string that appears in the codebase on one of the
tracked remotes and branches.

## Design

```
ELB -> ASG[ Web Frontends ] -> ElasticSearch <- codelauf worker -> sqlite
                            -> ZooKeeper     <-
```

there can be any number of web frontends, each of which is stateless.

a separate project provides the web front-end and API.

the web frontends provide an api that can be used to query the cluster state as it
is in zookeeper, and also to perform searches.

there is a single codelauf worker at any one time and this is enforced via zookeeper.
in future we could use leader election to allow failover, or partition the repositories
into buckets spread across a cluster of workers.

zookeeper is used for two things:
  1. long lived configuration data:
     1. list of repositories that need to be indexed
  2. ephemeral state of worker process:
     1. when it started
     2. what it's doing

codelauf stores mirrored git repositories on its local filesystem,
and also uses sqlite to track program state that should persist across application restarts,
but does not need to outlive the mirrored git repositories themselves.

if the worker machine is lost, it can be recovered by starting a new one and re-mirroring
the git repositories named in zookeeper. this process is automatic.
zookeeper also holds the indexed commit id of each branch as a backup, so no re-indexing is needed

if zookeeper is lost, its configuration will need to be recreated, and the codelauf worker
restarted.

if the elasticsearch cluster is lost, the worker will need to re-index everything.

it is recommended that if your repository setup is anything other than trivial, that you
create a script to drive the web api to add the repos automatically.

## Zookeeper file structure

```
/codelauf (root)
  /repositories
    /{43223-21998392-3232-123294}
      - type: git
        url: https://github.com/...
        branches:
	  - name: master
	    indexed_commit_id: blah
        last_indexed: Monday
        wanted_indexed: Tuesday
    /{09238-24234233-3242-432981}
      - type: hg?
        url: blah
        blah: blah
  /workers
    /0
      - start_time: Tuesday
      /repositories
        /{43223-21998392-3232-123294}
	  - status: cloning
	  - progress: 80%
	/{09238-24234233-3242-432981}
	  - status: indexing_files
	  - progress: 20%
```

## Frontend web API calls

```
/repositories index,get,patch,delete
/workers index,get
/search get
```

## Worker management API calls

note that there's no way to directly add or remove repos to a worker.
this is done via the worker watching zk /repositories at the moment.
this API is a bit redundant at the moment.
in future it will be used to coordinate ownership of repos among workers,

```
/repositories index,get
/repositories/{id}/sync post // trigger immediate fetch and sync
/repositories/{id}/recreate post // clone fresh copy and sync
/status get
```


## Worker design

### start

 1. open sqlite db
 2. create top-level nodes in zookeeper under /workers
 3. start watch on zk repositories node
 4. create nodes per project as per rows in sqlite db
 5. begin sync tasks:
    1. loop over projects defined in sqlite db
    2. for each watched remote start sync thread

### adding new project to sync

 1. create entry in sqlite
 2. start new sync thread

### sync thread

 1. find repo dir and check consistency against sqlite db:
 2. if dir doesn't exist, clone it
 3. if sqlite commit id doesn't exist in repo clear it
 4. git fetch all to manually sync with remote
 5. use revwalk to find all the commits back to the merge base(s):
    include in the revwalk all the repo's tracked branches in the branches table
    for each tracked branch:
      hide merge bases of (branch tip commit id, indexed commit id)
 6. add all commits found by revwalk to commits work table in sqlite
    crash recovery: ignore duplicate row errors
 7. scroll through commits work table and add each commit to elastic search
    mark row in work table as done
    periodically commit elasticsearch batch as we go
    all updates to search index are idempotent
    remove from search index any files deleted or renamed by a commit
    add to repo_files table any files that are added or updated
    if they're already in there then update the change commit id if newer
    crash recovery: no special logic needed. elasticsearch will eventually converge
 8. when all rows done, save each branch tip commit id as indexed commit id in branches table
    and clear work table.
    update each branch commit id in zookeeper
    crash recovery: update branches table and delete work table rows in same transaction.
    zookeeper branch commit id is eventually consistent.
 9. for each file in repo_files table, add to search index
    update repo_files indexed commit id as we go if change commit id is newer than indexed commit id
    crash recovery: it's monotonic. no special logic needed.

### sync thread states

 1. started
 2. start_fail couldn't open sqlite db or find data dir? or zk?
 3. cloning
 4. clone_fail couldn't access remote repo
 5. cloned
 6. fetching
 7. fetch_fail couldn't access remote repo
 8. fetched
 11. indexing_commits
 12. index_commits_fail error twiddling git or poking elasticsearch or sqlite
 13. indexed_commits
 14. indexing_files
 15. index_files_fail error poking elasticsearch or sqlite or git
 16. indexed_files


## SQLite db schema

### repositories table

 1. id uuid string (hyphen formatted, 36 chars)
 2. repo uri (e.g. https://github.com/me/foo.git)
 3. indexed_datetime for information only
 4. sync state (see above)
 5. local filesystem path

unique indexes on id and repo

### branches table

 1. repo_id
 2. name
 3. indexed_commit_id

unique index on (repo_id,name)

### commits work table

 1. id git oid of commit 20 char ascii
 2. repo_id uuid string of repo
 3. state enum indexed or not_indexed

unique index on (repo_id, id)

### repo_files table

 1. repo_id uuid string of repo
 2. path relative path in repo of file
 3. commit_id id of commit when last changed
 4. indexed_commit_id id of commit when last indexed
 
unique index on (repo_id, path)

## a note on paths, strings and unicode:

the rust code uses Paths where appropriate.
the sqlite db uses c strings.
converting between the two is done in modules::types.rs

there are no paths created from things that aren't already paths,
or are otherwise known to be something safe like ascii e.g.
a hash of the remote url is used instead of the url itself as
the dir to clone the repo into, and the branch names aren't used
in paths anywhere.

hopefully that's enough to be reasonably cross platform and
tolerant of non-utf8 inputs.