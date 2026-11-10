from domain import Job

class QueueService:
    def __init__(self, store):
        self.store = store

    def submit(self, key, payload):
        job = Job(key, payload)
        self.store.add(job)
        return job
