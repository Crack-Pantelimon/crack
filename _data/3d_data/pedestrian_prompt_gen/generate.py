#!/usr/bin/env python3
"""
Pedestrian prompt generator for GTA-style 3D model pipeline.

Reads variable files from variables/, randomly samples one option per category,
composes a full image-generation prompt, and writes 100 prompts to output/.
"""

from requests import structures
import os
import random
import shutil
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
VARIABLES_DIR = SCRIPT_DIR / "variables"
OUTPUT_DIR = SCRIPT_DIR / "output"
NUM_SAMPLES = 100

# Which variable files to load, in the order they appear in the template
VARIABLE_FILES = [
    "sex",
    "skin_color",
    "body_type",
    "individual_color",
    "top_clothes",
    "bottom_clothes",
    "shoes",
    "jewelry",
    "hat",
    "pose_info",
    "age_type",
    "culture_type",
    "money_type",
    "job_type",
]


def load_variables():
    """Load all variable files into a dict of lists."""
    variables = {}
    for name in VARIABLE_FILES:
        filepath = VARIABLES_DIR / f"{name}.txt"
        with open(filepath) as f:
            options = [line.strip() for line in f if line.strip()]
        if not options:
            raise ValueError(f"Variable file {filepath} is empty!")
        variables[name] = options
    return variables


def build_prompt(picks):
    """Build a single prompt string from a dict of picked values."""
    sex = picks["sex"]
    skin_color = picks["skin_color"]
    body_type = picks["body_type"]
    individual_color = picks["individual_color"]
    top_clothes = picks["top_clothes"]
    bottom_clothes = picks["bottom_clothes"]
    shoes = picks["shoes"]
    jewelry = picks["jewelry"]
    hat = picks["hat"]
    pose_info = picks["pose_info"]
    age_type = picks["age_type"]
    culture_type = picks["culture_type"]
    money_type = picks["money_type"]
    job_type = picks["job_type"]

    prompt = (
        f"A single {sex} in front of a gray cement brick wall with gray concrete floor, "
        f"This {sex} is a {job_type} and has something related to it on their body . "
        f"realistic studio lighting. "
        f"The person is wearing {top_clothes} and {bottom_clothes}, and {shoes}. "
        f"The person is wearing {jewelry}. "
        f"The person is wearing {hat}. "
        f"The person has a {body_type} and {individual_color}. "
        f"The person has {skin_color}. "
        f"The {sex} is {age_type} years old. "
        f"The person has a {culture_type} culture, origin and descent. "
        f"This {sex} is {money_type}. "
        f"{pose_info} "
    )
    return prompt


def main():
    # Delete and recreate output directory
    if OUTPUT_DIR.exists():
        shutil.rmtree(OUTPUT_DIR)
    OUTPUT_DIR.mkdir(parents=True)

    # Load all variable options
    variables = load_variables()

    print(f"Loaded {len(variables)} variable categories:")
    for name, options in variables.items():
        print(f"  {name}: {len(options)} options")


    output_file = OUTPUT_DIR / f"prompts.txt"
    with open(output_file, "w") as f:
    # Generate prompts
        for i in range(1, NUM_SAMPLES + 1):
            picks = {name: random.choice(options) for name, options in variables.items()}
            prompt = build_prompt(picks)

            f.write(prompt)
            f.write("\n")

    print(f"\nGenerated {NUM_SAMPLES} prompts in {output_file}")


if __name__ == "__main__":
    main()
