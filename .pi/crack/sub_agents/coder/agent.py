from crack_server.sub_agents.base import SubAgentPersona


class CoderPersona(SubAgentPersona):
    slug = "coder"
    name = "Coder"
    report_instructions = (
        "An implementation report listing every file changed, why, how to build/test, "
        "and any follow-ups. Do not claim work you did not perform."
    )
    templates = ["system.md", "nudge.md"]


PERSONA = CoderPersona()
