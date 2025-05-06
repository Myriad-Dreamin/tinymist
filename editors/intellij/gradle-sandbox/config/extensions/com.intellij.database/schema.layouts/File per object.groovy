import com.intellij.database.model.DasObjectWithSource
import com.intellij.database.model.DasSchemaChild

LAYOUT.ignoreDependencies = true
LAYOUT.baseName { ctx -> baseName(ctx.object) }


def baseName(obj) {
  for (def cur = obj; cur != null; cur = cur.dasParent) {
    if (storeSeparately(cur)) return sanitize(cur.name)
  }
  return sanitize(obj.name)
}

def storeSeparately(obj) {
  return obj instanceof DasObjectWithSource || obj instanceof DasSchemaChild
}

def sanitize(name) {
  return name.replace('/', 'slash')
}