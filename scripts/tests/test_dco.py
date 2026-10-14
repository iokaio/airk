# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("check_dco", Path(__file__).parents[1] / "check-dco.py")
dco = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dco)


class DcoContracts(unittest.TestCase):
    def commit(self, message):
        return {"commit": {"author": {"name": "Example Author", "email": "author@example.test"}, "message": message}}

    def test_contributor_and_coauthor_need_matching_final_trailers(self):
        trailer = "Signed-off-by: Example Author <author@example.test>"
        dco.validate(self.commit("Fix a fixture\n\n" + trailer))
        for message in ("Fix a fixture", "Title\n\n" + trailer + "\n\nQuoted body",
                        "Title\n\nSigned-off-by: Another Author <other@example.test>",
                        "Title\n\n" + trailer + "\nCo-authored-by: Example Coauthor <co@example.test>"):
            with self.assertRaises(ValueError):
                dco.validate(self.commit(message))
        dco.validate(self.commit("Title\n\n" + trailer + "\nCo-authored-by: Example Coauthor <co@example.test>\n"
                                 "Signed-off-by: Example Coauthor <co@example.test>"))


if __name__ == "__main__":
    unittest.main()
