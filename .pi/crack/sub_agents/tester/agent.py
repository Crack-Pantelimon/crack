from crack_server.sub_agents.base import SubAgentPersona


class TesterPersona(SubAgentPersona):
    slug = "tester"
    name = "Tester"
    report_instructions = (
        "A test report with commands run, pass/fail results, repro steps for failures, "
        "and recommended fixes. Include exact command lines and relevant log excerpts."
    )
    templates = ["system.md", "nudge.md"]


PERSONA = TesterPersona()
