class ArtifactStore:
    def __init__(self):
        self._items = {}

    def add(self, artifact):
        key = (artifact.name, artifact.version)
        if key in self._items:
            raise ValueError("duplicate artifact")
        self._items[key] = artifact

    def get(self, name, version):
        return self._items[name, version]
