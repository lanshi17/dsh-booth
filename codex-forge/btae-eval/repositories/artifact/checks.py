import unittest
from store import ArtifactStore
from service import ArtifactService

class ExistingContract(unittest.TestCase):
    def test_publish(self):
        service = ArtifactService(ArtifactStore())
        self.assertEqual(service.publish("a", 1, "body").content, "body")
        with self.assertRaises(ValueError):
            service.publish("a", 0, "body")

if __name__ == "__main__":
    unittest.main()
