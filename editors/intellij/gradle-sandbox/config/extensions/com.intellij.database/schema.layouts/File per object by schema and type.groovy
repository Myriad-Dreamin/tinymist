import com.intellij.database.model.DasObjectWithSource
import com.intellij.database.model.DasSchemaChild
import com.intellij.database.model.ObjectKind
import com.intellij.database.util.DasUtil
import com.intellij.database.util.ObjectPath

LAYOUT.ignoreDependencies = true
LAYOUT.baseName { ctx -> baseName(ctx.object) }
LAYOUT.fileScope { path -> fileScope(path) }


def baseName(obj) {
  def schema = DasUtil.getSchema(obj)
  def file = fileName(obj)
  if (schema.isEmpty()) {
    return file
  }
  else {
    return sanitize(schema) + "/" + obj.kind.code() + "/" + file
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
  return ObjectPath.create(root, ObjectKind.SCHEMA)
}

def storeSeparately(obj) {
  return obj instanceof DasObjectWithSource || obj instanceof DasSchemaChild
}

def sanitize(name) {
  return name.replace('/', 'slash')
}