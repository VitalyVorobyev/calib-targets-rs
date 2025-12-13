import cv2
import numpy as np

def export_dict(dict_id, name, n_markers, bits=4, border=1, cell=20):
    d = cv2.aruco.getPredefinedDictionary(dict_id)
    codes = []
    cells = bits + 2*border
    side = cells * cell

    for mid in range(n_markers):
        img = cv2.aruco.generateImageMarker(d, mid, side, borderBits=border)
        # img is 0..255, 0=black
        code = 0
        for by in range(bits):
            for bx in range(bits):
                cx = (bx + border) * cell + cell//2
                cy = (by + border) * cell + cell//2
                is_black = img[cy, cx] < 127
                bit = 1 if is_black else 0
                idx = by*bits + bx
                code |= (bit << idx)
        codes.append(code)

    print(f"// {name} ({bits}x{bits}), black=1, row-major")
    print(f"pub const {name}: [u16; {n_markers}] = [")
    for i,c in enumerate(codes):
        end = "," if i+1 < len(codes) else ""
        print(f"    0x{c:04x}{end}")
    print("];")

# 4x4_250:
export_dict(cv2.aruco.DICT_4X4_250, "DICT_4X4_250_CODES", 250)

# 4x4_1000:
export_dict(cv2.aruco.DICT_4X4_1000, "DICT_4X4_1000_CODES", 1000)
