app "astar"
    packages { base: "platform" }
    imports [base.Task]
    provides [ main ] to base

main : Task.Task {} []
main =
    6
        |> Str.fromInt
        |> Task.putLine

Model position :
    {
        evaluated : Set position,
        openSet : Set position,
        costs : Dict position F64,
        cameFrom : Dict position position
    }

initialModel : position -> Model position
initialModel = \start ->
    {
        evaluated : Set.empty, 
        openSet : Set.singleton start,
        costs : Dict.singleton start 0, 
        cameFrom : Dict.empty
    }

cheapestOpen : (position -> F64), Model position -> Result position {}
cheapestOpen = \costFn, model ->
    model.openSet
        |> Set.toList
        |> List.filterMap \position ->
            when Dict.get model.costs position is
                Err _ ->
                    Err {}

                Ok cost ->
                    Ok { position, cost: cost + costFn position }
        |> List.sortBy .cost
        |> List.first
        |> Result.map .position

reconstructPath : Dict position position, position -> List position
reconstructPath = \cameFrom, goal ->
    when Dict.get cameFrom goal is
        Err _ ->
            []

        Ok next ->
            List.append (reconstructPath cameFrom next) goal

# TODO shuffle things around so we get reuse
updateCost : position, position, Model position -> Model position
updateCost = \current, neighbor, model ->
    newCameFrom =
        Dict.insert model.cameFrom neighbor current

    newCosts =
        Dict.insert model.costs neighbor distanceTo

    distanceTo =
        reconstructPath newCameFrom neighbor
            |> List.len
            |> Num.toFloat

    newModel =
        { model & 
            costs: newCosts,
            cameFrom: newCameFrom
        }

    when Dict.get model.costs neighbor is
        Err _ ->
            newModel

        Ok previousDistance ->
            if distanceTo < previousDistance then
                newModel

            else
                model

astar : (position, position -> F64), (position -> Set position), position, Model position -> Result (List position) {}
astar = \costFn, moveFn, goal, model ->
    when cheapestOpen (\source -> costFn source goal) model is
        Err {} ->
            Err {}

        Ok current ->
            if current == goal then
                Ok (reconstructPath model.cameFrom goal)

            else
                modelPopped =
                    { model &
                        openSet: Set.remove model.openSet current,
                        evaluated: Set.insert model.evaluated current,
                    }

                neighbors =
                    moveFn current

                newNeighbors =
                    Set.difference modelPopped.evaluated neighbors

                modelWithNeighbors =
                    { modelPopped &
                        openSet: Set.union modelPopped.openSet newNeighbors
                    }

                modelWithCosts =
                    Set.walk newNeighbors (updateCost current) modelWithNeighbors

                astar costFn moveFn goal modelWithCosts

      
