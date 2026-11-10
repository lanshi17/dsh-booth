import unittest
from store import JobStore
from service import QueueService
import api

class ExistingContract(unittest.TestCase):
    def test_submit(self):
        service = QueueService(JobStore())
        self.assertEqual(api.submit(service, {"key":"a", "payload":"x"}), {"key":"a", "payload":"x"})
        with self.assertRaises(ValueError):
            service.submit("a", "x")

if __name__ == "__main__":
    unittest.main()
