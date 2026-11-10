from domain import Artifact

class ArtifactService:
    def __init__(self, store):
        self.store = store

    def publish(self, name, version, content):
        if version < 1:
            raise ValueError("positive version required")
        artifact = Artifact(name, version, content)
        self.store.add(artifact)
        return artifact
