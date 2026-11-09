# artifact diagnostic seed

Python standard-library-only synthetic small repository. Baseline checks: `python3 checks.py`.
Imports: api -> service -> domain; service uses a repository object. Concrete store is composed outside domain. Public behavior is identical for all study arms.
These seeds are engineering smoke fixtures, not independent real-project benchmarks.
