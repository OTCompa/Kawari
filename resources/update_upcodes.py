import json

diffDict = {}
seen_opcodes = {}
with open("diff.json", "r", encoding="utf-8") as diffFile:
    diff = json.load(diffFile)
    for opcode in diff:
        diffDict[int(opcode["old"][0], 0)] = int(opcode["new"][0], 0)

with open("opcodes.json", "r+", encoding="utf-8") as opcodesFile:
    opcodeDict = json.load(opcodesFile)
    for opcode_type, opcodes in opcodeDict.items():
        temp_array = []
        for opcode in opcodes:
            new_opcode = diffDict.get(opcode["opcode"])

            if new_opcode is not None:
                opcode["opcode"] = new_opcode

            if opcode["opcode"] in seen_opcodes:
                print(f"Duplicate opcode found for {opcode['name']} & {seen_opcodes[opcode['opcode']]}: {opcode["opcode"]}")
            seen_opcodes[opcode["opcode"]] = opcode["name"]

            temp_array.append(opcode)
        opcodeDict[opcode_type] = temp_array
    opcodesFile.seek(0)
    opcodesFile.write(json.dumps(opcodeDict, indent=4) + "\n")
