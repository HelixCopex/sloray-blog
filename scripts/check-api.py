#!/usr/bin/env python3
"""OpenAPI 漂移之外，拒绝删除接口和收紧既有请求/响应契约。"""
import argparse
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
VERBS = {"get", "post", "put", "patch", "delete", "head", "options"}


def compatible(old, new):
    errors = []
    def resolve(schema, document):
        if not isinstance(schema, dict) or "$ref" not in schema:
            return schema
        ref = schema["$ref"]
        if not ref.startswith("#/"):
            raise ValueError("外部 schema 引用不能用于离线契约比较")
        result = document
        for part in ref[2:].split("/"):
            result = result[part.replace("~1", "/").replace("~0", "~")]
        return result

    visited = set()
    def schema(before, after, direction, location):
        before, after = resolve(before, old), resolve(after, new)
        if not isinstance(before, dict) or not isinstance(after, dict):
            if before != after: errors.append(f"{location}: 布尔 schema 变化")
            return
        identity = (id(before), id(after), direction)
        if identity in visited:
            return
        visited.add(identity)
        btypes, atypes = before.get("type"), after.get("type")
        if btypes != atypes:
            errors.append(f"{location}: 类型变化 {btypes} → {atypes}")
        if "enum" in before:
            if "enum" not in after:
                if direction == "response":
                    errors.append(f"{location}: 响应 enum 约束丢失")
            else:
                left, right = (before["enum"], after["enum"]) if direction == "request" else (after["enum"], before["enum"])
                if not all(value in right for value in left):
                    errors.append(f"{location}: enum 不兼容")
        elif "enum" in after and direction == "request":
            errors.append(f"{location}: 新请求 enum 约束")
        breq, areq = set(before.get("required", [])), set(after.get("required", []))
        if (areq - breq if direction == "request" else breq - areq):
            errors.append(f"{location}: 必填字段不兼容")
        for key, field in before.get("properties", {}).items():
            target = after.get("properties", {}).get(key)
            if target is None:
                errors.append(f"{location}.{key}: 字段删除")
            else:
                schema(field, target, direction, f"{location}.{key}")
        if "items" in before:
            if "items" not in after:
                errors.append(f"{location}: items 丢失")
            else:
                schema(before["items"], after["items"], direction, location + "[]")
        for key in ("oneOf", "anyOf", "allOf", "not", "additionalProperties", "pattern", "format", "minimum", "maximum", "minLength", "maxLength"):
            if before.get(key) != after.get(key):
                errors.append(f"{location}: {key} 约束变化，需人工确认新契约版本")

    def content(before, after, direction, location):
        for mime, value in before.items():
            if mime not in after:
                errors.append(f"{location}: {mime} 删除")
            elif "schema" in value:
                schema(value["schema"], after[mime].get("schema", {}), direction, location)

    for path, item in old.get("paths", {}).items():
        for method in VERBS & item.keys():
            location = f"{method.upper()} {path}"
            target = new.get("paths", {}).get(path, {}).get(method)
            if not target:
                errors.append(f"{location}: 接口删除")
                continue
            operation = item[method]
            def parameters(document, path_item, op):
                result = {}
                for value in path_item.get('parameters', []) + op.get('parameters', []):
                    p = resolve(value, document)
                    result[(p['in'], p['name'])] = p
                return result
            parameters = parameters(old, item, operation)
            new_parameters = {}
            for value in new['paths'][path].get('parameters', []) + target.get('parameters', []):
                p = resolve(value, new)
                new_parameters[(p['in'],p['name'])] = p
            if operation.get('security', old.get('security', [])) != target.get('security', new.get('security', [])):
                errors.append(f"{location}: 认证要求变化")
            for key, parameter in parameters.items():
                if key not in new_parameters:
                    errors.append(f"{location}: 参数 {key} 删除")
                else:
                    if not parameter.get("required") and new_parameters[key].get("required"):
                        errors.append(f"{location}: 参数 {key} 变为必填")
                    schema(parameter.get("schema", {}), new_parameters[key].get("schema", {}), "request", location + str(key))
            for key, parameter in new_parameters.items():
                if key not in parameters and parameter.get("required"):
                    errors.append(f"{location}: 新必填参数 {key}")
            before_body, after_body = resolve(operation.get("requestBody", {}), old), resolve(target.get("requestBody", {}), new)
            if not before_body.get("required") and after_body.get("required"):
                errors.append(f"{location}: requestBody 变为必填")
            content(before_body.get("content", {}), after_body.get("content", {}), "request", location + " request")
            for code, response in operation.get("responses", {}).items():
                if code not in target.get("responses", {}):
                    errors.append(f"{location}: 响应状态 {code} 删除")
                else:
                    content(resolve(response, old).get("content", {}), resolve(target["responses"][code], new).get("content", {}), "response", location + " " + code)
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline")
    parser.add_argument("--base-ref")
    parser.add_argument("--validate", action="store_true")
    args = parser.parse_args()
    new = json.loads((ROOT / "docs/api/openapi.json").read_text())
    identifiers = set()
    def validate(value):
        if isinstance(value, dict):
            if '$ref' in value:
                ref = value['$ref']
                if not ref.startswith('#/'): raise ValueError('契约只能含本地引用')
                target = new
                for part in ref[2:].split('/'): target = target[part.replace('~1','/').replace('~0','~')]
            for item in value.values(): validate(item)
        elif isinstance(value, list):
            for item in value: validate(item)
    validate(new)
    for path, item in new.get('paths', {}).items():
        for method in VERBS & item.keys():
            operation_id = item[method].get('operationId')
            if not operation_id or operation_id in identifiers: raise ValueError('operationId 缺失或重复：'+str(operation_id))
            identifiers.add(operation_id)
    if args.validate:
        print('PASS OpenAPI 本地引用与唯一 operationId')
        return
    if args.baseline:
        old = json.loads(pathlib.Path(args.baseline).read_text())
    elif args.base_ref:
        subprocess.run(["git", "cat-file", "-e", args.base_ref + "^{commit}"], cwd=ROOT, check=True, capture_output=True)
        result = subprocess.run(["git", "show", f"{args.base_ref}:docs/api/openapi.json"], cwd=ROOT, capture_output=True, text=True)
        if result.returncode:
            print("基线没有 OpenAPI 文件（首次引入契约）")
            return
        old = json.loads(result.stdout)
    else:
        raise SystemExit("需要 --baseline 或 --base-ref")
    errors = compatible(old, new)
    for error in errors:
        print("FAIL " + error, file=sys.stderr)
    if errors:
        raise SystemExit(1)
    print("PASS OpenAPI 向后兼容性检查")


if __name__ == "__main__":
    main()
