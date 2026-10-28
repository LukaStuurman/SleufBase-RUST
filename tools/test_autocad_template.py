import unittest
from verify_autocad_template import inspect_output


class AutoCADAcceptanceTests(unittest.TestCase):
    def test_populated_drawing_with_clean_audits(self):
        result = inspect_output('Total errors found 0 fixed 0\nSLEUFBASE_ENTITIES=802\nTotal errors found 0 fixed 0')
        self.assertEqual(result, {'entities': 802, 'audits': 2})

    def test_discarded_dxf_is_failure_even_when_blank_drawing_audits_clean(self):
        for error in ['Error in LAYER Table', 'DXF read error on line 593674.', 'Invalid or incomplete DXF input -- drawing discarded.']:
            with self.subTest(error=error), self.assertRaises(RuntimeError):
                inspect_output(error + '\nTotal errors found 0 fixed 0\nSLEUFBASE_ENTITIES=1')

    def test_missing_entities_or_audit_never_pass(self):
        for output in ['Total errors found 0 fixed 0', 'SLEUFBASE_ENTITIES=802', 'Total errors found 0 fixed 0\nSLEUFBASE_ENTITIES=0']:
            with self.subTest(output=output), self.assertRaises(RuntimeError):
                inspect_output(output)

    def test_audit_repairs_are_reported_as_failure(self):
        with self.assertRaises(RuntimeError):
            inspect_output('Total errors found 1 fixed 1\nSLEUFBASE_ENTITIES=802')


if __name__ == '__main__':
    unittest.main()
