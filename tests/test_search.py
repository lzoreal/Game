import os
import sys
import unittest
from datetime import datetime

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from hydra_search import SearchController, SearchTicket, sort_by_date


def dated(value):
    return {"date_dt": value}


class SearchControllerTests(unittest.TestCase):
    def test_observe_and_begin_issue_tickets(self):
        controller = SearchController(debounce_ms=150)
        self.assertTrue(controller.observe(" Cyber "))
        self.assertEqual(controller.query, "Cyber")

        first = controller.begin("Cyber", "alpha", True, 0.4, now=10.0)
        self.assertIsInstance(first, SearchTicket)
        self.assertEqual(
            (first.query, first.source_filter, first.fuzzy, first.threshold),
            ("Cyber", "alpha", True, 0.4),
        )
        self.assertTrue(controller.accepts(first.token))

        controller.observe("Cyberpunk")
        self.assertFalse(controller.accepts(first.token))
        second = controller.begin("Cyberpunk", "", False, 0.2, now=20.0)
        self.assertTrue(controller.accepts(second.token))
        self.assertFalse(controller.accepts(first.token))

    def test_empty_queries_are_not_searchable(self):
        controller = SearchController()
        self.assertFalse(controller.observe("   "))
        self.assertIsNone(controller.begin("", now=1.0))
        self.assertEqual(controller.token, 2)

    def test_elapsed_ms_uses_the_ticket_clock(self):
        controller = SearchController()
        ticket = controller.begin("game", now=100.0)
        self.assertAlmostEqual(controller.elapsed_ms(ticket, now=100.25), 250.0)

    def test_date_sort_defaults_to_newest_and_keeps_undated_last(self):
        controller = SearchController()
        newest = dated(datetime(2024, 3, 1))
        middle = dated(datetime(2024, 2, 1))
        oldest = dated(datetime(2024, 1, 1))
        undated = dated(None)
        groups = [oldest, undated, newest, middle]

        self.assertEqual(
            controller.apply_sort(groups), [newest, middle, oldest, undated]
        )
        self.assertEqual(groups[0], oldest)

        controller.toggle_date_sort()
        self.assertEqual(
            controller.apply_sort(groups), [oldest, middle, newest, undated]
        )

        controller.toggle_date_sort()
        self.assertEqual(
            controller.apply_sort(groups), [newest, middle, oldest, undated]
        )

    def test_sort_by_date_does_not_mutate_input(self):
        newest = dated(datetime(2024, 3, 1))
        oldest = dated(datetime(2024, 1, 1))
        groups = [oldest, newest]
        self.assertEqual(sort_by_date(groups, True), [newest, oldest])
        self.assertEqual(groups, [oldest, newest])


if __name__ == "__main__":
    unittest.main()
