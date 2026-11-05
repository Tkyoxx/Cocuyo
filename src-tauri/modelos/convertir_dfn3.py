# saca los pesos de DeepFilterNet3 (los tres onnx oficiales) a dfn3.bin, a media precision, con nombres cortos
# uso: python convertir_dfn3.py <carpeta con enc.onnx, erb_dec.onnx y df_dec.onnx>

import struct
import sys
from pathlib import Path

import numpy as np
import onnx
from onnx import numpy_helper


def short(output: str) -> str:
    return output.strip("/").replace("_output_0", "").replace("/", ".")


def tensors(path: Path, prefix: str):
    graph = onnx.load(str(path)).graph
    known = {i.name: numpy_helper.to_array(i) for i in graph.initializer}
    for node in graph.node:
        if node.op_type == "Constant":
            for a in node.attribute:
                if a.name == "value":
                    known[node.output[0]] = numpy_helper.to_array(a.t)
    out = {}
    for node in graph.node:
        name = f"{prefix}.{short(node.output[0])}"
        weights = [known[i] for i in node.input[1:] if i in known]
        if node.op_type in ("Conv", "ConvTranspose", "Einsum", "MatMul"):
            out[name + ".w"] = weights[0]
            if len(weights) > 1:
                out[name + ".b"] = weights[1]
        elif node.op_type == "GRU":
            w, r, b = (known[node.input[i]] for i in (1, 2, 3))
            out[name + ".W"], out[name + ".R"], out[name + ".B"] = w[0], r[0], b[0]
        elif node.op_type == "Add":
            bias = [known[i] for i in node.input if i in known and known[i].ndim == 1 and i.endswith(".bias")]
            if bias:
                out[name + ".b"] = bias[0]
    return out


def main():
    folder = Path(sys.argv[1])
    found = {}
    for file, prefix in (("enc.onnx", "enc"), ("erb_dec.onnx", "erb"), ("df_dec.onnx", "df")):
        found.update(tensors(folder / file, prefix))
    blob = bytearray(b"DFN3")
    blob += struct.pack("<I", len(found))
    total = 0
    for name, value in found.items():
        value = np.asarray(value, dtype=np.float32)
        raw = name.encode()
        blob += struct.pack("<H", len(raw)) + raw
        blob += struct.pack("<B", value.ndim) + b"".join(struct.pack("<I", d) for d in value.shape)
        blob += value.astype("<f2").tobytes()
        total += value.size
        print(f"{name:55s} {list(value.shape)}")
    Path(__file__).with_name("dfn3.bin").write_bytes(bytes(blob))
    print(f"{len(found)} tensores, {total} pesos, {len(blob)} bytes")


main()
