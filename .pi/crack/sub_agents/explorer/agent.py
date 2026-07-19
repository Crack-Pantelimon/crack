from crack_server.sub_agents.base import SubAgentPersona


class ExplorerPersona(SubAgentPersona):
    slug = "explorer"
    name = "Explorer"
    report_instructions = (
        "A concise exploration report: scope investigated, key files and symbols found, "
        "open questions, risks, and recommended next steps with concrete paths."
    )
    templates = ["system.md", "nudge.md"]


PERSONA = ExplorerPersona()
