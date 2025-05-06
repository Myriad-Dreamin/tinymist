import com.intellij.database.model.DasObjectWithSource
import com.intellij.database.model.DasSchemaChild
import com.intellij.database.model.ObjectKind
import com.intellij.database.util.DasUtil
import com.intellij.database.util.ObjectPath

LAYOUT.ignoreDependencies = true
LAYOUT.baseName { ctx -> baseName(ctx.object) }
LAYOUT.fileScope { path -> fileScope(path) }


def baseName(obj) {
  def db = DasUtil.getCatalog(obj)
  def schema = DasUtil.getSchema(obj)
  def file = fileName(obj)
  if (db.isEmpty()) {
    if (!schema.isEmpty()) return "anonymous/" + sanitize(schema) + "/" + file
    return file
  }
  else if (schema.isEmpty()) {
    return sanitize(db) + "/" + file
  }
  else {
    return sanitize(db) + "/" + sanitize(schema) + "/" + file
  }
}

def fileName(obj) {
  for (def cur = obj; cur != null; cur = cur.dasParent) {
    if (storeSeparately(cur)) return sanitize(cur.name)
  }
  return sanitize(obj.name)
}

def fileScope(path) {
  def root = path.getName(0).toString()
  if (root.endsWith(".sql")) return null
  def next = path.getName(1).toString()
  if (next.endsWith(".sql")) {
    if (root == "anonymous") return null
    return ObjectPath.create(root, ObjectKind.DATABASE)
  }
  if (root == "anonymous") return ObjectPath.create(next, ObjectKind.SCHEMA)
  return ObjectPath.create(root, ObjectKind.DATABASE).append(next, ObjectKind.SCHEMA)
}

def storeSeparately(obj) {
  return obj instanceof DasObjectWithSource || obj instanceof DasSchemaChild
}

def sanitize(name) {
  return name.replace('/', 'slash')
}