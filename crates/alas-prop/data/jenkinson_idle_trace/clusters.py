import sys
import numpy as np
import pypdfium2 as pdfium
path, page, scale, r0, r1 = sys.argv[1], int(sys.argv[2]), float(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5])
cols = [int(c) for c in sys.argv[6:]]
img = pdfium.PdfDocument(path)[page-1].render(scale=scale).to_pil().convert("L")
m = np.array(img) < 128
for x in cols:
    col = m[r0:r1, x]
    out, st = [], None
    for i, v in enumerate(col):
        if v and st is None: st = i
        if (not v) and st is not None:
            out.append((st + r0, i - 1 + r0)); st = None
    print(x, out)
