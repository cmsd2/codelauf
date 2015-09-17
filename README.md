Codelauf is a source code search system

It is a work-in-progress.
This design document describes how it will be architected.


Codelauf mirrors git repositories and uses elasticsearch to index files and commits on tracked branches.

Code is passed through some language specific syntax analysers before being loaded into the index.

You can search the indexes given a commit id or a string that appears in the codebase on one of the
tracked remotes and branches.

design:

```
ELB -> ASG[ Web Frontends ] -> ElasticSearch <- codelauf worker -> sqlite
                            -> ZooKeeper     <-
```

there can be any number of web frontends, each of which is stateless.

a separate project provides the web front-end and API.

the web frontends provide an api that can be used to query the cluster state as it
is in zookeeper, and also to perform searches.

there is a single codelauf worker at any one time and this is enforced via zookeeper.
in future we could use leader election to allow failover.

zookeeper is used for two things:
  long lived configuration data:
    list of repositories that need to be indexed
  ephemeral state of worker process:
    when it started
    what it's doing

codelauf stores mirrored git repositories on its local filesystem,
and also uses sqlite to track program state that should persist across application restarts,
but does not need to outlive the mirrored git repositories themselves.

if the worker machine is lost, it can be recovered by starting a new one and re-mirroring
the git repositories named in zookeeper. this process is automatic.

if zookeeper is lost, its configuration will need to be recreated, and the codelauf worker
restarted.

if the elasticsearch cluster is lost, the worker will need to re-index everything.

it is recommended that if your repository setup is anything other than trivial, that you
create a script to drive the web api to add the repos automatically.

zookeeper file structure:
/codelauf (root)
  /repositories
    /{43223-21998392-3232-123294}
      - type: git
        url: https://github.com/...
        branch: master
        last_indexed: Monday
        wanted_indexed: Tuesday
    /{09238-24234233-3242-432981}
      - type: hg?
        url: blah
        blah: blah
  /workers
    /0
      - start_time: Tuesday
      /status
        /{43223-21998392-3232-123294}
	  - status: cloning
	  - progress: 80%
	/{09238-24234233-3242-432981}
	  - status: indexing_files
	  - progress: 20%

web API calls:

/repositories index,get,patch,delete
/workers index,get
/search get


Worker design:

start
open sqlite db
create top-level nodes in zookeeper under /workers
start watch on zk repositories node
create nodes per project as per rows in sqlite db
begin sync tasks:
loop over projects defined in sqlite db
for each watched remote start sync thread

adding new project to sync:
create entry in sqlite
start new sync thread

sync thread:
find repo dir and check consistency against sqlite db:
if dir doesn't exist, clone it
if sqlite commit id doesn't exist in repo clear it
git fetch all to manually sync with remote
if local and remote branches have diverged, find latest commit that we have in common,
  and delete from the search index all local commits since then
now we can fast forward through the remote commits and add them to the search index,
  updating sqlite with the processed commit id as we go
any files that were deleted would have been removed from the index when processing commits
spider the entire repo and add all the files to the index, replacing any existing docs in index

sync thread states:
started
start_fail couldn't open sqlite db or find data dir? or zk?
cloning
clone_fail couldn't access remote repo
fetching
fetch_fail couldn't access remote repo
rewinding
rewind_fail error twiddling git or poking elasticsearch
merging
merge_fail error twiddling git or poking elasticsearch
indexing
index_fail error poking elasticsearch
indexed


