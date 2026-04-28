import re
text = open('resume/RC_CurriculumVitae_2026.md', 'r', encoding='utf-8').read()
text = text.replace('<div style="margin-left: 48pt">\n\n# RYAN M. CLANTON PhD - CURRICULUM VITAE', '<div align="center">\n\n# RYAN M. CLANTON PhD - CURRICULUM VITAE')
open('resume/RC_CurriculumVitae_2026.md', 'w', encoding='utf-8').write(text)

