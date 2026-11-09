from domain import Job

class JobStore:
    def __init__(self):
        self._jobs = {}

    def add(self, job: Job):
        if job.key in self._jobs:
            raise ValueError("duplicate job")
        self._jobs[job.key] = job

    def get(self, key):
        return self._jobs[key]
