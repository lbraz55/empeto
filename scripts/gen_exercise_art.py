#!/usr/bin/env python3
"""Gera as ilustrações próprias dos exercícios (assets/exercises/*.svg). Uso: scripts/gen_exercise_art.py assets/exercises"""
import os, sys
OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)
B = 3.6   # traço do corpo
E = 2.4   # traço de equipamento
def line(pts, w=B):
    return '<polyline points="%s" fill="none" stroke="#000" stroke-width="%s" stroke-linecap="round" stroke-linejoin="round"/>' % (" ".join(f"{x},{y}" for x, y in pts), w)
def head(x, y, r=4.2):
    return f'<circle cx="{x}" cy="{y}" r="{r}" fill="#000"/>'
def circ(x, y, r, w=E):
    return f'<circle cx="{x}" cy="{y}" r="{r}" fill="none" stroke="#000" stroke-width="{w}"/>'
def plate(x, y, r=5):
    return circ(x, y, r) + f'<circle cx="{x}" cy="{y}" r="1.4" fill="#000"/>'
def db(x, y, horiz=True):  # halter pequeno
    if horiz:
        return line([(x-4, y), (x+4, y)], E) + line([(x-4, y-2.5), (x-4, y+2.5)], E) + line([(x+4, y-2.5), (x+4, y+2.5)], E)
    return line([(x, y-4), (x, y+4)], E) + line([(x-2.5, y-4), (x+2.5, y-4)], E) + line([(x-2.5, y+4), (x+2.5, y+4)], E)
def ground(y=58, a=6, b=58):
    return line([(a, y), (b, y)], 1.6)
def front(hx=32, top=12, arms=None, legs=((26, 58), (38, 58)), hip=38):
    s = head(hx, top) + line([(hx, top+6), (hx, hip)])
    s += line([(legs[0][0]+2, (hip+legs[0][1])//2 + 0), legs[0]]) if False else ''
    s += line([legs[0], (hx-1, hip), (hx+1, hip), legs[1]])
    if arms:
        for a in arms:
            s += line(a)
    return s
art = {}
art["bench-press"] = (line([(10, 40), (48, 40)], E) + line([(14, 40), (14, 52)], E) + line([(44, 40), (44, 52)], E)
    + head(12, 33) + line([(17, 36), (38, 36)]) + line([(38, 36), (46, 40), (48, 56)])
    + line([(22, 36), (22, 21)]) + plate(22, 18) + ground(57, 30, 58))
art["push-up"] = (ground(50) + head(12, 30) + line([(16, 33), (36, 39), (54, 46)]) + line([(18, 34), (18, 49)]))
art["fly"] = (head(32, 12) + line([(32, 18), (32, 38)]) + line([(26, 58), (31, 38), (33, 38), (38, 58)])
    + line([(32, 21), (16, 25), (27, 31)]) + line([(32, 21), (48, 25), (37, 31)]) + db(27, 33) + db(37, 33))
art["pull-up"] = (line([(8, 7), (56, 7)], E) + head(32, 18) + line([(23, 7), (19, 19), (26, 24), (38, 24), (45, 19), (41, 7)])
    + line([(32, 24), (32, 42)]) + line([(28, 57), (31, 42), (33, 42), (36, 57)]))
art["pulldown"] = (circ(32, 5, 2.5) + line([(32, 7), (32, 14)], 1.6) + line([(12, 14), (52, 14)], E) + head(32, 22)
    + line([(20, 14), (18, 25), (25, 29), (39, 29), (46, 25), (44, 14)]) + line([(32, 28), (32, 44)])
    + line([(22, 46), (42, 46)], E) + line([(26, 58), (26, 47), (31, 44), (33, 44), (38, 47), (38, 58)]))
art["row"] = (ground() + head(16, 22) + line([(21, 26), (40, 36)]) + line([(40, 36), (38, 47), (41, 58)])
    + line([(24, 27), (32, 32), (27, 41)]) + plate(27, 44, 4.5))
art["overhead-press"] = (line([(10, 6), (54, 6)], E) + line([(10, 3), (10, 9)], E) + line([(54, 3), (54, 9)], E)
    + head(32, 15) + line([(20, 6), (17, 17), (25, 22), (39, 22), (47, 17), (44, 6)]) + line([(32, 21), (32, 39)])
    + line([(26, 58), (31, 39), (33, 39), (38, 58)]))
art["dip"] = (line([(18, 34), (18, 60)], E) + line([(46, 34), (46, 60)], E) + line([(14, 34), (22, 34)], E) + line([(42, 34), (50, 34)], E)
    + head(32, 11) + line([(18, 34), (16, 24), (25, 19), (39, 19), (48, 24), (46, 34)]) + line([(32, 18), (32, 38)])
    + line([(29, 52), (27, 46), (31, 38), (33, 38), (37, 46), (35, 52)]))
art["lateral-raise"] = (head(32, 12) + line([(9, 23), (24, 20), (40, 20), (55, 23)]) + db(7, 23, False) + db(57, 23, False)
    + line([(32, 18), (32, 38)]) + line([(26, 58), (31, 38), (33, 38), (38, 58)]))
art["curl"] = (ground() + head(30, 10) + line([(30, 16), (30, 36)]) + line([(28, 58), (30, 36), (32, 58)])
    + line([(30, 19), (31, 31), (41, 23)]) + db(42, 21, False))
art["triceps-extension"] = (circ(46, 5, 2.5) + line([(46, 7), (39, 37)], 1.6) + ground() + head(28, 10)
    + line([(28, 16), (28, 36)]) + line([(26, 58), (28, 36), (30, 58)]) + line([(28, 19), (29, 31), (39, 38)]) + line([(36, 38), (42, 38)], E))
art["squat"] = (ground() + plate(27, 21, 5.5) + head(35, 14) + line([(31, 21), (20, 40)]) + line([(20, 40), (37, 42), (32, 58)]))
art["leg-press"] = (line([(26, 58), (58, 26)], 1.6) + line([(8, 28), (16, 50), (30, 50)], E) + head(11, 23) + line([(13, 29), (22, 47)]) + line([(22, 47), (34, 30), (46, 37)]) + line([(43, 25), (50, 47)], E) + line([(15, 33), (24, 40)]))
art["leg-extension"] = (line([(16, 40), (36, 40)], E) + line([(16, 40), (16, 20)], E) + line([(26, 40), (26, 56)], E) + ground(57, 14, 40)
    + head(21, 13) + line([(21, 19), (22, 38)]) + line([(22, 38), (36, 38), (52, 32)]) + circ(53, 35, 3) + line([(21, 22), (27, 30), (30, 38)]))
art["lunge"] = (ground() + head(30, 12) + line([(30, 18), (30, 37)]) + line([(44, 58), (42, 46), (31, 37), (29, 37), (20, 54), (12, 55)])
    + line([(30, 21), (31, 34)]) + db(31, 37))
art["deadlift"] = (ground() + head(43, 21) + line([(39, 26), (22, 35)]) + line([(22, 35), (32, 47), (34, 58)])
    + line([(37, 27), (38, 46)]) + plate(38, 51, 6))
art["leg-curl"] = (line([(6, 40), (46, 40)], E) + line([(12, 40), (12, 54)], E) + line([(40, 40), (40, 54)], E) + head(9, 33)
    + line([(14, 36), (34, 36), (44, 36), (47, 22)]) + circ(48, 19, 3) + line([(15, 37), (18, 40)]))
art["hip-thrust"] = (line([(4, 36), (16, 36)], E) + line([(8, 36), (8, 50)], E) + ground(50) + head(9, 29)
    + line([(14, 32), (32, 28)]) + line([(32, 28), (44, 32), (46, 49)]) + plate(32, 23, 5) + line([(16, 32), (24, 30), (30, 26)]))
art["hip-abduction"] = (line([(22, 40), (42, 40)], E) + head(32, 12) + line([(32, 18), (32, 38)])
    + line([(15, 57), (17, 44), (31, 38), (33, 38), (47, 44), (49, 57)]) + line([(13, 40), (13, 48)], E) + line([(51, 40), (51, 48)], E)
    + line([(32, 21), (24, 32), (24, 39)]) + line([(32, 21), (40, 32), (40, 39)]))
art["calf-raise"] = (line([(24, 54), (44, 54), (44, 58), (24, 58), (24, 54)], E) + head(33, 9) + line([(33, 15), (33, 33)]) + line([(32, 49), (33, 33), (34, 49)]) + line([(32, 49), (37, 54)]) + line([(34, 49), (38, 54)]) + line([(33, 19), (33, 31)]) + line([(18, 50), (18, 38)], E) + line([(14, 42), (18, 37), (22, 42)], E))
art["plank"] = (ground(50) + head(13, 37) + line([(26, 50), (17, 50), (18, 41), (38, 43), (56, 49)]))
art["crunch"] = (ground(50) + head(14, 33) + line([(18, 37), (30, 48)]) + line([(30, 48), (40, 36), (48, 49)])
    + line([(18, 37), (13, 30)]) + line([(13, 30), (11, 34)], 2.4))
art["leg-raise"] = (ground(50) + head(9, 46) + line([(14, 48), (34, 48), (44, 22)]) + line([(16, 48), (22, 44)]))
art["cardio-run"] = (line([(6, 58), (56, 58)], E) + line([(50, 58), (56, 36)], E) + head(34, 10) + line([(32, 16), (28, 34)])
    + line([(14, 46), (21, 44), (28, 34), (38, 41), (35, 54)]) + line([(31, 19), (40, 25), (45, 19)]) + line([(31, 19), (24, 25), (20, 31)]))
art["cardio-bike"] = (circ(42, 48, 9) + line([(42, 48), (34, 30), (22, 30)], E) + line([(34, 30), (44, 22), (48, 22)], E)
    + line([(30, 40), (34, 30)], E) + head(36, 9) + line([(33, 15), (24, 28)]) + line([(33, 16), (46, 23)])
    + line([(24, 28), (35, 37), (34, 49)]))
art["generic"] = (line([(16, 32), (48, 32)], 4) + line([(16, 22), (16, 42)], 5) + line([(10, 26), (10, 38)], 5)
    + line([(48, 22), (48, 42)], 5) + line([(54, 26), (54, 38)], 5))
for k, body in art.items():
    open(os.path.join(OUT, k + ".svg"), "w").write(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
        '<!-- EMPETO: ilustração própria (MIT, mesmo licenciamento do projeto) -->' + body + '</svg>\n')
print(len(art))
