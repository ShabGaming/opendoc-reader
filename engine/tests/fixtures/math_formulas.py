"""LaTeX strings copied from the spike's recognition output for the math page
(G:/opendoc-spike/results/bench/pvl-cuda/math.md and ds1-q8-cuda/math.md), kept as fixtures
so the converter tests need no external files."""

PADDLE_DISPLAY = [
    "-\\frac{1}{m}=-\\frac{dx_{1}}{dy_{1}}.",
    "y-y_{1}=-\\frac{dx_{1}}{dy_{1}}(x-x_{1}).",
    "TM^{*}=\\frac{MP_{1}}{\\tan\\tau}=y_{1}\\frac{dx_{1}}{dy_{1}}=length~of~subtangent.",
    "MN^{\\dagger}=MP_{1}\\tan\\tau=y_{1}\\frac{dy_{1}}{dx_{1}}=length~of~subnormal.",
    "\\begin{aligned}TP_{1}=\\sqrt{\\overline{TM}^{2}+\\overline{MP}_{1}^{2}}&=\\sqrt{\\left(y_{1}\\frac{dx_{1}}{dy_{1}}\\right)^{2}+\\left(y_{1}\\right)^{2}}\\\\&=y_{1}\\sqrt{\\left(\\frac{dx_{1}}{dy_{1}}\\right)^{2}+1}=length~of~tangent.\\end{aligned}",
    "\\begin{aligned}P_{1}N&=\\sqrt{\\overline{MP_{1}}^{2}+\\overline{MN}^{2}}=\\sqrt{(y_{1})^{2}+\\left(y_{1}\\frac{dy_{1}}{dx_{1}}\\right)^{2}}\\\\&=y_{1}\\sqrt{1+\\left(\\frac{dy_{1}}{dx_{1}}\\right)^{2}}=length~of~normal.\\end{aligned}",
]

PADDLE_INLINE = [
    "P_{1}(x_{1}, y_{1})",
    "P_{1}N",
    "OX",
    "=TP_1",
    "X",
    "=TM",
    "=P_1N",
    "=MN",
    "TP_{1}M",
    "\\tan \\tau = \\frac{MP_{1}}{TM}",
    "MP_{1}N",
    "\\tan\\tau=\\frac{MN}{MP_{1}}",
    "TP_{1}",
    "=P_{1}N",
]

DEEPSEEK_DISPLAY = [
    "-\\frac{1}{m} = -\\frac{dx_1}{dy_1}. \\quad (By 55, p. 3)",
    "(2) \\qquad y - y_1 = -\\frac{dx_1}{dy_1} (x - x_1).",
    "{T P_{1}=\\sqrt{T M^{2}+M P_{1}^{2}}=\\sqrt{\\left(y_{1}\\frac{d x_{1}}{d y_{1}}\\right)^{2}+\\left(y_{1}\\right)^{2}}}",
    "{=y_{1}\\sqrt{\\left(\\frac{d x_{1}}{d y_{1}}\\right)^{2}+1}=\\text{length of tangent.}}",
    "{P_{1}N=\\sqrt{M P_{1}^{2}+M N^{2}}=\\sqrt{\\left(y_{1}\\right)^{2}+\\left(y_{1}\\frac{d y_{1}}{d x_{1}}\\right)^{2}}}",
    "{=y_{1}\\sqrt{1+\\left(\\frac{d y_{1}}{d x_{1}}\\right)^{2}}=\\text{length of normal.}}",
]

DEEPSEEK_INLINE = [
    "P_1(x_1, y_1)",
    "P_1N",
    "OX",
    "(= TP_1)",
    "X",
    "(= TM)",
    "(= P_1N)",
    "(= MN)",
    "TP_1M",
    "\\tan \\tau = \\frac{MP_1}{TM}",
    "TM^* = \\frac{MP_1}{\\tan \\tau} = y_1 \\frac{dx_1}{dy_1} = \\text{length of subtangent.}",
    "MP_1N",
    "\\tan \\tau = \\frac{MN}{MP_1}",
    "MN^\\dagger = MP_1 \\tan \\tau = y_1 \\frac{dy_1}{dx_1} = \\text{length of subnormal.}",
    "(= TP_1)",
    "(= P_1N)",
]
