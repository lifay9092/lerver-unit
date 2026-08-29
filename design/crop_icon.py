"""裁掉图标源图边缘的水印并居中输出为 icon-source.png"""
from PIL import Image

SRC = r"D:\RustProject\LerverUnit\lever-unit\design\Modern_flat_app_icon_for_a_sof_2026-08-29T08-36-58.png"
DST = r"D:\RustProject\LerverUnit\lever-unit\design\icon-source.png"

img = Image.open(SRC).convert("RGBA")
w, h = img.size
print("source size:", w, h)

# 四周各裁 6%，去除右下角水印；裁剪后再缩放回 1024x1024 方形
crop = int(w * 0.06)
img2 = img.crop((crop, crop, w - crop, h - crop))
img2 = img2.resize((1024, 1024), Image.LANCZOS)
img2.save(DST)
print("saved:", DST)
